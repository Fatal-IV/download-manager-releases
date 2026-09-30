//! Bağlantı sayısı hız testi: aynı dosyayı 1, 4, 8, 16… bağlantıyla kısa süre indirip hızı ölçer.
//! Veri diske yazılmaz; yalnızca sayılır. Böylece "kaç bağlantı işe yarıyor" ölçümle yanıtlanır.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::header::RANGE;
use reqwest::{Client, StatusCode};
use serde::Serialize;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::engine::probe;

/// Bundan küçük dosyalarda ölçüm anlamlı olmaz (bağlantı kurulumu baskın çıkar).
pub const MIN_TOTAL: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LevelState {
    Running,
    Done,
    /// Sunucu bu kadar bağlantıyı reddetti (429).
    Limited,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    pub connections: u32,
    pub state: LevelState,
    /// Bayt/sn.
    pub speed: f64,
    pub detail: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub levels: Vec<u32>,
    /// Bağlantı kurulumu ve TCP yavaş başlangıcı ölçüme katılmasın diye atlanan süre.
    pub warmup: Duration,
    pub measure: Duration,
    /// Seviyeler arası bekleme; sunucu hız sınırlayıcıları dinlensin.
    pub cooldown: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            levels: vec![1, 4, 8, 16],
            warmup: Duration::from_secs(2),
            measure: Duration::from_secs(6),
            cooldown: Duration::from_secs(1),
        }
    }
}

enum ConnEnd {
    Finished,
    Throttled,
    Failed(String),
}

#[allow(clippy::too_many_arguments)]
async fn one_connection(
    client: Client,
    url: String,
    headers: Vec<(String, String)>,
    start: u64,
    counter: Arc<AtomicU64>,
    deadline: Instant,
    cancel: CancellationToken,
) -> ConnEnd {
    let mut req = client.get(&url).header(RANGE, format!("bytes={start}-"));
    for (k, v) in &headers {
        req = req.header(k, v);
    }
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => return ConnEnd::Failed(e.to_string()),
    };
    match resp.status() {
        StatusCode::PARTIAL_CONTENT => {}
        StatusCode::TOO_MANY_REQUESTS => return ConnEnd::Throttled,
        s => return ConnEnd::Failed(format!("Sunucu {} döndürdü", s.as_u16())),
    }
    let mut stream = resp.bytes_stream();
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return ConnEnd::Finished,
            _ = tokio::time::sleep_until(deadline.into()) => return ConnEnd::Finished,
            next = stream.next() => match next {
                None => return ConnEnd::Finished,
                Some(Err(e)) => return ConnEnd::Failed(e.to_string()),
                Some(Ok(chunk)) => { counter.fetch_add(chunk.len() as u64, Ordering::Relaxed); }
            }
        }
    }
}

async fn measure_level(
    client: &Client,
    url: &str,
    headers: &[(String, String)],
    total: u64,
    n: u32,
    cfg: &Config,
    cancel: &CancellationToken,
    emit: &(dyn Fn(Update) + Send + Sync),
) -> Update {
    let counter = Arc::new(AtomicU64::new(0));
    let t0 = Instant::now();
    let deadline = t0 + cfg.warmup + cfg.measure;
    let stop = cancel.child_token();

    let mut set = JoinSet::new();
    for i in 0..u64::from(n) {
        set.spawn(one_connection(
            client.clone(),
            url.to_string(),
            headers.to_vec(),
            total * i / u64::from(n),
            counter.clone(),
            deadline,
            stop.clone(),
        ));
    }

    let mut mark: Option<(u64, Instant)> = None;
    let mut speed = 0.0;
    let mut problem: Option<(LevelState, Option<String>)> = None;
    let mut tick = tokio::time::interval(Duration::from_millis(400));

    while !set.is_empty() {
        tokio::select! {
            _ = tick.tick() => {
                let now = Instant::now();
                let bytes = counter.load(Ordering::Relaxed);
                if mark.is_none() && now.duration_since(t0) >= cfg.warmup {
                    mark = Some((bytes, now));
                }
                speed = match mark {
                    Some((b0, t)) if now > t => bytes.saturating_sub(b0) as f64 / now.duration_since(t).as_secs_f64(),
                    _ => bytes as f64 / now.duration_since(t0).as_secs_f64().max(0.001),
                };
                emit(Update { connections: n, state: LevelState::Running, speed, detail: None });
            }
            res = set.join_next() => {
                match res {
                    Some(Ok(ConnEnd::Throttled)) if problem.is_none() => {
                        problem = Some((LevelState::Limited, Some("Sunucu bu kadar bağlantıyı reddetti (429)".into())));
                        stop.cancel();
                    }
                    Some(Ok(ConnEnd::Failed(e))) if problem.is_none() => {
                        problem = Some((LevelState::Failed, Some(e)));
                        stop.cancel();
                    }
                    _ => {}
                }
            }
        }
    }

    if let Some((state, detail)) = problem {
        return Update { connections: n, state, speed: 0.0, detail };
    }
    // Son hız: ısınmadan sonra inen bayt / geçen süre.
    let end = Instant::now();
    if let Some((b0, t)) = mark {
        let secs = end.duration_since(t).as_secs_f64();
        if secs > 0.0 {
            speed = counter.load(Ordering::Relaxed).saturating_sub(b0) as f64 / secs;
        }
    }
    Update { connections: n, state: LevelState::Done, speed, detail: None }
}

/// Testi çalıştırır; her seviye için canlı (`Running`) ve nihai güncellemeleri `emit` ile bildirir.
pub async fn run(
    client: &Client,
    url: &str,
    headers: &[(String, String)],
    cfg: &Config,
    cancel: CancellationToken,
    emit: Arc<dyn Fn(Update) + Send + Sync>,
) -> Result<Vec<Update>, String> {
    let info = probe(client, url, headers).await.map_err(|e| e.to_string())?;
    let total = info.total.filter(|_| info.accepts_ranges).ok_or(
        "Sunucu parça indirmeyi desteklemiyor; bağlantı sayısı bu dosyada hız farkı yaratmaz.",
    )?;
    if total < MIN_TOTAL {
        return Err("Test için en az 32 MB'lık bir dosya gerekir.".into());
    }

    let mut results = Vec::new();
    for (i, &n) in cfg.levels.iter().enumerate() {
        if cancel.is_cancelled() {
            break;
        }
        if i > 0 {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = tokio::time::sleep(cfg.cooldown) => {}
            }
        }
        let update = measure_level(client, url, headers, total, n, cfg, &cancel, &*emit).await;
        emit(update.clone());
        let stop_here = update.state != LevelState::Done;
        results.push(update);
        // Sunucu bir seviyede reddettiyse daha yükseklerini denemenin anlamı yok.
        if stop_here {
            break;
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::testserver::{spawn, Cfg};

    fn fast() -> Config {
        Config {
            levels: vec![1, 4],
            warmup: Duration::from_millis(400),
            measure: Duration::from_millis(1500),
            cooldown: Duration::from_millis(100),
        }
    }

    fn noop() -> Arc<dyn Fn(Update) + Send + Sync> {
        Arc::new(|_| {})
    }

    #[tokio::test]
    async fn more_connections_measure_faster_when_server_limits_per_connection() {
        // Her bağlantı ~64 KiB / 20 ms ≈ 3 MB/sn ile sınırlı.
        let srv = spawn(Cfg { total: 64 * 1024 * 1024, throttle_ms: 20, ..Cfg::default() }).await;
        let out = run(&Client::new(), &srv.url, &[], &fast(), CancellationToken::new(), noop()).await.unwrap();
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|u| u.state == LevelState::Done));
        assert!(
            out[1].speed > out[0].speed * 2.5,
            "1 bağlantı: {:.0}, 4 bağlantı: {:.0}",
            out[0].speed,
            out[1].speed
        );
    }

    #[tokio::test]
    async fn reports_limited_when_server_rejects_connections() {
        let srv = spawn(Cfg { total: 64 * 1024 * 1024, throttle_ms: 5, max_concurrent: 2, ..Cfg::default() }).await;
        let out = run(&Client::new(), &srv.url, &[], &fast(), CancellationToken::new(), noop()).await.unwrap();
        assert_eq!(out[0].state, LevelState::Done);
        assert_eq!(out[1].state, LevelState::Limited);
    }

    #[tokio::test]
    async fn rejects_files_too_small_to_measure() {
        let srv = spawn(Cfg { total: 1024 * 1024, ..Cfg::default() }).await;
        let err = run(&Client::new(), &srv.url, &[], &fast(), CancellationToken::new(), noop()).await.unwrap_err();
        assert!(err.contains("32 MB"));
    }

    /// Gerçek ağda elle çalıştırmak için: `SPEEDTEST_URL=... cargo test --lib real_network -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn real_network() {
        let url = std::env::var("SPEEDTEST_URL").expect("SPEEDTEST_URL gerekli");
        let emit: Arc<dyn Fn(Update) + Send + Sync> = Arc::new(|u| {
            if u.state != LevelState::Running {
                println!("{} bağlantı: {:?} {:.2} MB/sn {:?}", u.connections, u.state, u.speed / 1e6, u.detail);
            }
        });
        let out = run(&Client::new(), &url, &[], &Config::default(), CancellationToken::new(), emit).await;
        println!("{:?}", out.map(|v| v.len()));
    }
}
