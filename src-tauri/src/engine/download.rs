use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{IF_RANGE, RANGE};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use tokio::fs::OpenOptions;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use super::error::EngineError;
use super::filename;
use super::probe::probe;
use super::segment::{self, Segment};

const MAX_RETRIES: u32 = 5;
/// 429 alan parça bu kadar kez beklenip yeniden denenir; her seferinde eşzamanlı bağlantı sayısı azalır.
const MAX_THROTTLE_RETRIES: u32 = 20;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

/// İndirmeyi tanımlayan, değişmeyen girdiler.
#[derive(Debug, Clone)]
pub struct JobSpec {
    pub url: String,
    pub dest_dir: PathBuf,
    /// Verilmezse sunucunun bildirdiği ya da URL'den çıkarılan ad kullanılır.
    pub filename: Option<String>,
    pub connections: u32,
    /// Çerez, Referer, User-Agent gibi ek başlıklar (tarayıcı eklentisinden gelir).
    pub headers: Vec<(String, String)>,
    /// Aynı dosyanın yansı adresleri. Boyutu ve Range desteği eşleşenler parçaları paylaşır.
    /// Çerez gibi başlıklar yalnızca ana adrese gönderilir.
    pub mirrors: Vec<String>,
    /// Ana adres birden fazla IP'ye çözülüyorsa (CDN) bağlantıları bu IP'lere dağıt.
    pub spread_ips: bool,
    /// Tüm indirmelerin paylaştığı hız sınırlayıcı; yoksa sınırsız.
    pub limiter: Option<Arc<super::Limiter>>,
}

/// Bir parçanın sunucuyla yaptığı gerçek konuşmanın kaydı; parçaların gerçekten ayrı bağlantılarla
/// indiğinin kanıtıdır.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SegMeta {
    /// Gönderilen `Range` başlığı.
    pub range: String,
    /// Sunucunun döndürdüğü HTTP kodu (parça için 206 beklenir); 0 = henüz yanıt yok.
    pub http_status: u16,
    /// Sunucunun yanıttaki `Content-Range` başlığı.
    pub content_range: Option<String>,
    /// Bağlantının kurulduğu sunucu adresi (IP:port).
    pub remote: Option<String>,
    /// Parçanın indirildiği kaynak adres (ana adres ya da yansı).
    pub source: String,
    /// Bu parça için kurulan bağlantı sayısı (1 = ilk denemede, fazlası yeniden deneme/devam).
    pub connections_opened: u32,
}

fn snapshot(meta: &[Mutex<SegMeta>]) -> Vec<SegMeta> {
    meta.iter().map(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone()).collect()
}

/// Duraklatıp devam etmek için saklanması gereken her şey.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobState {
    pub total: Option<u64>,
    pub accepts_ranges: bool,
    pub segments: Vec<Segment>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub part_path: PathBuf,
    pub final_path: PathBuf,
    /// Doğrulanmış kaynaklar; ilki ana adres. Eski kayıtlarda boş olabilir.
    #[serde(default)]
    pub sources: Vec<String>,
}

impl JobState {
    pub fn downloaded(&self) -> u64 {
        self.segments.iter().map(|s| s.downloaded).sum()
    }

    /// Parça parça `Range` ile indirilebilir mi?
    fn ranged(&self) -> bool {
        self.accepts_ranges && self.total.is_some()
    }

    /// `If-Range` için doğrulayıcı. Zayıf ETag'ler (`W/`) bu başlıkta geçersizdir.
    fn validator(&self) -> Option<String> {
        match &self.etag {
            Some(e) if !e.starts_with("W/") => Some(e.clone()),
            _ => self.last_modified.clone(),
        }
    }

    fn is_complete(&self) -> bool {
        self.total.is_some() && self.segments.iter().all(Segment::is_done)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Completed(PathBuf),
    Paused,
    Failed(String),
}

/// Sonuç ne olursa olsun güncel durum döner; böylece duraklatılan ya da hata alan iş sürdürülebilir.
#[derive(Debug, Clone)]
pub struct RunResult {
    pub state: JobState,
    pub outcome: Outcome,
}

fn with_part_ext(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

/// `ad.ext` doluysa `ad (1).ext`, `ad (2).ext`… üretir. `.part` dosyaları da dolu sayılır.
fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let taken = |p: &Path| p.exists() || with_part_ext(p).exists();
    let first = dir.join(name);
    if !taken(&first) {
        return first;
    }
    let p = Path::new(name);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or(name);
    let ext = p.extension().and_then(|s| s.to_str());
    (1u32..)
        .map(|n| match ext {
            Some(e) => dir.join(format!("{stem} ({n}).{e}")),
            None => dir.join(format!("{stem} ({n})")),
        })
        .find(|c| !taken(c))
        .expect("sonsuz aralıkta boş ad bulunur")
}

/// Sunucuyu yoklar, parçaları planlar ve `.part` dosyasını önceden ayırır.
pub(super) fn retry_after_of(resp: &reqwest::Response) -> Option<u64> {
    retry_after(resp)
}

pub async fn prepare(client: &Client, spec: &JobSpec) -> Result<JobState, EngineError> {
    let info = probe(client, &spec.url, &spec.headers).await?;
    let name = match &spec.filename {
        Some(n) => filename::sanitize(n),
        None => info.filename.clone(),
    };
    tokio::fs::create_dir_all(&spec.dest_dir).await?;
    let final_path = unique_path(&spec.dest_dir, &name);
    let part_path = with_part_ext(&final_path);

    let segments = match info.total {
        Some(t) if info.accepts_ranges => segment::plan(t, spec.connections),
        Some(t) => segment::plan(t, 1),
        // Boyut bilinmiyor: tek yer tutucu parça; gerçek boyut indirme bitince öğrenilir.
        None => vec![Segment { start: 0, end: 0, downloaded: 0 }],
    };

    // Yansıları sınarız; yalnızca aynı boyutta ve Range destekleyenler kullanılır.
    let mut sources = vec![spec.url.clone()];
    if info.accepts_ranges && info.total.is_some() {
        for m in &spec.mirrors {
            if sources.contains(m) {
                continue;
            }
            if let Ok(mi) = probe(client, m, &[]).await {
                if mi.accepts_ranges && mi.total == info.total {
                    sources.push(m.clone());
                }
            }
        }
    }

    let file = tokio::fs::File::create(&part_path).await?;
    if let Some(t) = info.total {
        file.set_len(t).await?;
    }

    Ok(JobState {
        total: info.total,
        accepts_ranges: info.accepts_ranges,
        segments,
        etag: info.etag,
        last_modified: info.last_modified,
        part_path,
        final_path,
        sources,
    })
}

/// Adı birden fazla IP'ye çözülen sunucu için her IP'ye sabitlenmiş istemciler üretir.
/// IPv4 varsa yalnızca IPv4 kullanılır (ulaşılamayan IPv6 adreslerine takılmamak için).
async fn pinned_clients(url: &str) -> Vec<Client> {
    let Ok(u) = reqwest::Url::parse(url) else { return Vec::new() };
    let (Some(host), Some(port)) = (u.host_str(), u.port_or_known_default()) else { return Vec::new() };
    if host.parse::<std::net::IpAddr>().is_ok() {
        return Vec::new();
    }
    let Ok(addrs) = tokio::net::lookup_host((host, port)).await else { return Vec::new() };
    let mut ips: Vec<std::net::IpAddr> = addrs.map(|a| a.ip()).collect();
    ips.sort();
    ips.dedup();
    if ips.iter().any(|ip| ip.is_ipv4()) {
        ips.retain(|ip| ip.is_ipv4());
    }
    ips.truncate(4);
    if ips.len() < 2 {
        return Vec::new();
    }
    ips.into_iter()
        .filter_map(|ip| {
            Client::builder()
                .resolve(host, std::net::SocketAddr::new(ip, port))
                .connect_timeout(Duration::from_secs(20))
                .build()
                .ok()
        })
        .collect()
}

async fn build_sources(client: &Client, spec: &JobSpec, state: &JobState) -> Vec<Source> {
    let urls = if state.sources.is_empty() { vec![spec.url.clone()] } else { state.sources.clone() };
    let ranged = state.ranged();
    let mut out = Vec::new();
    for (i, url) in urls.into_iter().enumerate() {
        let mut clients = if spec.spread_ips && ranged { pinned_clients(&url).await } else { Vec::new() };
        if clients.is_empty() {
            clients.push(client.clone());
        }
        let primary = i == 0;
        out.push(Source {
            url,
            headers: if primary { spec.headers.clone() } else { Vec::new() },
            clients,
            validator: if primary { state.validator() } else { None },
        });
    }
    out
}

/// Görev başına paylaşılan, değişmeyen bağlam.
/// Bir indirme kaynağı: adres, ona özel başlıklar ve bağlantı kurulacak istemciler.
struct Source {
    url: String,
    headers: Vec<(String, String)>,
    /// CDN dağıtımında her biri farklı bir IP'ye sabitlenmiş istemciler; yoksa tek istemci.
    clients: Vec<Client>,
    /// `If-Range` doğrulayıcısı; yalnızca ana adres için.
    validator: Option<String>,
}

struct Shared {
    sources: Vec<Source>,
    part_path: PathBuf,
    ranged: bool,
    total: Option<u64>,
    /// Devam ediliyorsa sunucuya `If-Range` ile kaynağın değişmediğini doğrulatırız.
    resuming: bool,
    segments: Vec<Segment>,
    meta: Vec<Mutex<SegMeta>>,
    /// Aynı anda açık olabilecek bağlantı sayısı; sunucu 429 verirse azalır.
    slots: Arc<Semaphore>,
    cap: AtomicUsize,
    limiter: Option<Arc<super::Limiter>>,
}

/// İndirmeyi çalıştırır. `cancel` tetiklenirse indirilen kısım korunarak `Paused` döner.
pub async fn run(
    client: &Client,
    spec: &JobSpec,
    mut state: JobState,
    cancel: CancellationToken,
    on_progress: Arc<dyn Fn(&[u64], &[SegMeta]) + Send + Sync>,
) -> RunResult {
    // Range desteklemeyen sunucuda kaldığı yerden devam edilemez; baştan başlarız.
    if !state.accepts_ranges {
        for s in &mut state.segments {
            s.downloaded = 0;
        }
    }

    let counters: Arc<Vec<AtomicU64>> =
        Arc::new(state.segments.iter().map(|s| AtomicU64::new(s.downloaded)).collect());
    let sources = build_sources(client, spec, &state).await;
    let shared = Arc::new(Shared {
        sources,
        part_path: state.part_path.clone(),
        ranged: state.ranged(),
        total: state.total,
        resuming: state.downloaded() > 0,
        segments: state.segments.clone(),
        meta: state.segments.iter().map(|_| Mutex::new(SegMeta::default())).collect(),
        slots: Arc::new(Semaphore::new(state.segments.len().max(1))),
        cap: AtomicUsize::new(state.segments.len().max(1)),
        limiter: spec.limiter.clone(),
    });

    // Bir parça hata verirse kardeşlerini durdurmak için ayrı bir alt belirteç.
    let stop = cancel.child_token();
    let mut set = JoinSet::new();
    for idx in 0..state.segments.len() {
        if state.total.is_some() && state.segments[idx].is_done() {
            continue;
        }
        set.spawn(fetch_segment(shared.clone(), idx, counters.clone(), stop.clone()));
    }

    let reporter_stop = CancellationToken::new();
    let reporter = tokio::spawn({
        let counters = counters.clone();
        let shared = shared.clone();
        let stop = reporter_stop.clone();
        let cb = on_progress.clone();
        async move {
            loop {
                tokio::select! {
                    _ = stop.cancelled() => break,
                    _ = tokio::time::sleep(PROGRESS_INTERVAL) => {
                        let snap: Vec<u64> = counters.iter().map(|c| c.load(Ordering::Relaxed)).collect();
                        cb(&snap, &snapshot(&shared.meta));
                    }
                }
            }
        }
    });

    let mut failure: Option<String> = None;
    while let Some(res) = set.join_next().await {
        let err = match res {
            Ok(Ok(())) => continue,
            Ok(Err(e)) => e.to_string(),
            Err(join) => join.to_string(),
        };
        failure.get_or_insert(err);
        stop.cancel();
    }
    reporter_stop.cancel();
    let _ = reporter.await;

    for (seg, c) in state.segments.iter_mut().zip(counters.iter()) {
        seg.downloaded = c.load(Ordering::Relaxed);
    }
    on_progress(
        &state.segments.iter().map(|s| s.downloaded).collect::<Vec<_>>(),
        &snapshot(&shared.meta),
    );

    // Boyut bilinmiyorsa akışın sorunsuz bitmesi tamamlanma demektir.
    if state.total.is_none() && failure.is_none() && !cancel.is_cancelled() {
        let n = state.downloaded();
        state.total = Some(n);
        state.segments = if n == 0 { Vec::new() } else { vec![Segment { start: 0, end: n - 1, downloaded: n }] };
    }

    let outcome = if let Some(msg) = failure {
        Outcome::Failed(msg)
    } else if state.is_complete() {
        match finalize(&mut state).await {
            Ok(path) => Outcome::Completed(path),
            Err(e) => Outcome::Failed(e.to_string()),
        }
    } else if cancel.is_cancelled() {
        Outcome::Paused
    } else {
        Outcome::Failed("İndirme eksik bitti".to_string())
    };

    RunResult { state, outcome }
}

/// `.part` dosyasını son adına taşır.
async fn finalize(state: &mut JobState) -> Result<PathBuf, EngineError> {
    // Hazırlıktan bu yana aynı ad alınmış olabilir; ezmeyelim.
    if state.final_path.exists() {
        let dir = state.final_path.parent().unwrap_or(Path::new("."));
        let name = state.final_path.file_name().and_then(|n| n.to_str()).unwrap_or("indirme");
        state.final_path = unique_path(dir, name);
    }
    tokio::fs::rename(&state.part_path, &state.final_path).await?;
    Ok(state.final_path.clone())
}

/// Tek bir parçayı indirir. Geçici hatalarda üstel geri çekilmeyle yeniden dener; sunucu 429 verirse
/// eşzamanlı bağlantı sayısını azaltıp `Retry-After` kadar bekler.
async fn fetch_segment(
    sh: Arc<Shared>,
    idx: usize,
    counters: Arc<Vec<AtomicU64>>,
    cancel: CancellationToken,
) -> Result<(), EngineError> {
    let mut attempt = 0;
    let mut throttled = 0;
    // Kaynak kayması: bir yansı başarısız olursa parça sıradaki kaynağa geçer.
    let mut shift = 0;
    // Her yeniden denemede IP'yi de değiştiririz.
    let mut failures = 0usize;
    loop {
        if cancel.is_cancelled() {
            return Ok(());
        }
        let permit = tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            p = sh.slots.clone().acquire_owned() => match p {
                Ok(p) => p,
                Err(_) => return Ok(()),
            },
        };
        let n = sh.sources.len();
        let src = &sh.sources[(idx + shift) % n];
        let client = &src.clients[(idx / n + failures) % src.clients.len()];
        let wait = match try_once(&sh, src, client, idx, &counters[idx], &cancel).await {
            Ok(()) => return Ok(()),
            Err(EngineError::Throttled { retry_after }) if throttled < MAX_THROTTLE_RETRIES => {
                throttled += 1;
                // Bu izni geri vermeyiz: sunucu bu kadar bağlantıyı kaldırmıyor. En az 1 bağlantı kalır.
                let shrunk = sh.cap.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |c| (c > 1).then(|| c - 1));
                if shrunk.is_ok() {
                    permit.forget();
                } else {
                    drop(permit);
                }
                Duration::from_secs(retry_after.unwrap_or(2u64.pow(throttled.min(4))).clamp(1, 60))
            }
            Err(e) if attempt < MAX_RETRIES && e.is_retryable() => {
                attempt += 1;
                failures += 1;
                drop(permit);
                Duration::from_millis(250 * 2u64.pow(attempt - 1))
            }
            Err(e) if shift + 1 < n => {
                // Bu kaynak vazgeçirdi; parçayı sıradaki kaynaktan dene.
                let _ = e;
                shift += 1;
                attempt = 0;
                drop(permit);
                Duration::from_millis(200)
            }
            Err(e) => return Err(e),
        };
        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            _ = tokio::time::sleep(wait) => {}
        }
    }
}

/// `Retry-After` saniye cinsindense okur (tarih biçimi desteklenmez).
fn retry_after(resp: &reqwest::Response) -> Option<u64> {
    resp.headers().get(reqwest::header::RETRY_AFTER)?.to_str().ok()?.trim().parse().ok()
}

async fn try_once(
    sh: &Shared,
    src: &Source,
    client: &Client,
    idx: usize,
    counter: &AtomicU64,
    cancel: &CancellationToken,
) -> Result<(), EngineError> {
    let seg = sh.segments[idx];
    let mut req = client.get(&src.url);
    for (k, v) in &src.headers {
        req = req.header(k, v);
    }

    let start = if sh.ranged {
        let done = counter.load(Ordering::Relaxed);
        if done >= seg.len() {
            return Ok(());
        }
        let start = seg.start + done;
        req = req.header(RANGE, format!("bytes={start}-{}", seg.end));
        if done > 0 || sh.resuming {
            if let Some(v) = &src.validator {
                req = req.header(IF_RANGE, v);
            }
        }
        start
    } else {
        // Range yok: her denemede baştan yazarız.
        counter.store(0, Ordering::Relaxed);
        0
    };

    let resp = req.send().await?;
    let status = resp.status();
    if status == StatusCode::TOO_MANY_REQUESTS {
        return Err(EngineError::Throttled { retry_after: retry_after(&resp) });
    }
    {
        let mut m = sh.meta[idx].lock().unwrap_or_else(|e| e.into_inner());
        m.range = if sh.ranged { format!("bytes={start}-{}", seg.end) } else { "(tümü)".into() };
        m.http_status = status.as_u16();
        m.content_range = resp
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        m.remote = resp.remote_addr().map(|a| a.to_string());
        m.source = src.url.clone();
        m.connections_opened += 1;
    }
    if sh.ranged {
        if status != StatusCode::PARTIAL_CONTENT {
            // 200 gelmesi, If-Range'in tutmadığını, yani kaynağın değiştiğini gösterir.
            return Err(if status.is_success() {
                EngineError::ResourceChanged
            } else {
                EngineError::Status(status.as_u16())
            });
        }
    } else if !status.is_success() {
        return Err(EngineError::Status(status.as_u16()));
    }

    let mut file = OpenOptions::new().write(true).open(&sh.part_path).await?;
    file.seek(SeekFrom::Start(start)).await?;
    let mut stream = resp.bytes_stream();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                file.flush().await?;
                return Ok(());
            }
            next = stream.next() => match next {
                None => break,
                Some(Err(e)) => {
                    file.flush().await?;
                    return Err(e.into());
                }
                Some(Ok(chunk)) => {
                    let mut data: &[u8] = &chunk;
                    if sh.ranged {
                        // Sunucu istenenden fazla gönderirse komşu parçanın alanına taşmayalım.
                        let remaining = seg.len() - counter.load(Ordering::Relaxed);
                        if remaining == 0 {
                            break;
                        }
                        if data.len() as u64 > remaining {
                            data = &data[..remaining as usize];
                        }
                    }
                    if let Some(l) = &sh.limiter {
                        // Beklerken duraklatılırsa hemen çık; akış okunmadıkça TCP de yavaşlar.
                        tokio::select! {
                            _ = l.acquire(data.len()) => {}
                            _ = cancel.cancelled() => {
                                file.flush().await?;
                                return Ok(());
                            }
                        }
                    }
                    file.write_all(data).await?;
                    counter.fetch_add(data.len() as u64, Ordering::Relaxed);
                }
            }
        }
    }
    file.flush().await?;

    if let Some(total) = sh.total {
        let expected = if sh.ranged { seg.len() } else { total };
        if counter.load(Ordering::Relaxed) < expected {
            return Err(EngineError::Truncated);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::testserver::{pattern, spawn, Cfg};
    use super::*;

    fn spec(url: String, dir: &Path, connections: u32) -> JobSpec {
        JobSpec { url, dest_dir: dir.to_path_buf(), filename: None, connections, headers: vec![], mirrors: vec![], spread_ips: false, limiter: None }
    }

    fn noop() -> Arc<dyn Fn(&[u64], &[SegMeta]) + Send + Sync> {
        Arc::new(|_, _| {})
    }

    #[tokio::test]
    async fn segments_report_real_range_requests() {
        let srv = spawn(Cfg { total: 4 * 1024 * 1024, ..Cfg::default() }).await;
        let dir = tempfile::tempdir().unwrap();
        let client = Client::new();
        let sp = spec(srv.url.clone(), dir.path(), 4);
        let state = prepare(&client, &sp).await.unwrap();
        let last: Arc<Mutex<Vec<SegMeta>>> = Arc::default();
        let sink = last.clone();
        let cb: Arc<dyn Fn(&[u64], &[SegMeta]) + Send + Sync> =
            Arc::new(move |_, m| *sink.lock().unwrap() = m.to_vec());
        let res = run(&client, &sp, state.clone(), CancellationToken::new(), cb).await;
        assert!(matches!(res.outcome, Outcome::Completed(_)));
        let meta = last.lock().unwrap().clone();
        assert_eq!(meta.len(), 4);
        for (m, seg) in meta.iter().zip(&state.segments) {
            assert_eq!(m.http_status, 206);
            assert_eq!(m.range, format!("bytes={}-{}", seg.start, seg.end));
            assert!(m.content_range.as_deref().unwrap().starts_with(&format!("bytes {}-{}/", seg.start, seg.end)));
            assert!(m.remote.is_some());
            assert_eq!(m.connections_opened, 1);
        }
    }

    #[tokio::test]
    async fn backs_off_when_server_limits_connections() {
        // Sunucu aynı anda en fazla 2 bağlantıya izin veriyor; 8 istiyoruz.
        let srv = spawn(Cfg { total: 4 * 1024 * 1024, throttle_ms: 5, max_concurrent: 2, ..Cfg::default() }).await;
        let dir = tempfile::tempdir().unwrap();
        let client = Client::new();
        let sp = spec(srv.url.clone(), dir.path(), 8);
        let state = prepare(&client, &sp).await.unwrap();
        let res = run(&client, &sp, state, CancellationToken::new(), noop()).await;
        let Outcome::Completed(path) = res.outcome else { panic!("{:?}", res.outcome) };
        assert_eq!(std::fs::read(path).unwrap(), pattern(4 * 1024 * 1024));
        assert!(srv.peak_concurrent() <= 2);
    }

    #[tokio::test]
    async fn parts_are_shared_across_valid_mirrors_and_bad_mirrors_are_dropped() {
        let total = 4 * 1024 * 1024;
        let main = spawn(Cfg { total, ..Cfg::default() }).await;
        let good = spawn(Cfg { total, ..Cfg::default() }).await;
        let wrong_size = spawn(Cfg { total: total + 5, ..Cfg::default() }).await;
        let dir = tempfile::tempdir().unwrap();
        let client = Client::new();
        let mut sp = spec(main.url.clone(), dir.path(), 4);
        sp.mirrors = vec![good.url.clone(), wrong_size.url.clone()];
        let state = prepare(&client, &sp).await.unwrap();
        assert_eq!(state.sources, vec![main.url.clone(), good.url.clone()]);

        let last: Arc<Mutex<Vec<SegMeta>>> = Arc::default();
        let sink = last.clone();
        let cb: Arc<dyn Fn(&[u64], &[SegMeta]) + Send + Sync> =
            Arc::new(move |_, m| *sink.lock().unwrap() = m.to_vec());
        let res = run(&client, &sp, state, CancellationToken::new(), cb).await;
        let Outcome::Completed(path) = res.outcome else { panic!("{:?}", res.outcome) };
        assert_eq!(std::fs::read(path).unwrap(), pattern(total));
        let meta = last.lock().unwrap().clone();
        assert!(meta.iter().any(|m| m.source == main.url));
        assert!(meta.iter().any(|m| m.source == good.url));
    }

    #[tokio::test]
    async fn falls_back_to_main_when_a_mirror_fails() {
        let total = 2 * 1024 * 1024;
        let main = spawn(Cfg { total, ..Cfg::default() }).await;
        let flaky = spawn(Cfg { total, fail_range_requests: 1000, ..Cfg::default() }).await;
        let dir = tempfile::tempdir().unwrap();
        let client = Client::new();
        let mut sp = spec(main.url.clone(), dir.path(), 4);
        sp.mirrors = vec![flaky.url.clone()];
        let state = prepare(&client, &sp).await.unwrap();
        let res = run(&client, &sp, state, CancellationToken::new(), noop()).await;
        let Outcome::Completed(path) = res.outcome else { panic!("{:?}", res.outcome) };
        assert_eq!(std::fs::read(path).unwrap(), pattern(total));
    }

    #[tokio::test]
    async fn probe_reports_size_ranges_and_filename() {
        let srv = spawn(Cfg { total: 1_000_000, ..Cfg::default() }).await;
        let info = probe(&Client::new(), &srv.url, &[]).await.unwrap();
        assert_eq!(info.total, Some(1_000_000));
        assert!(info.accepts_ranges);
        assert_eq!(info.filename, "test file.bin");
        assert!(info.etag.is_some());
    }

    #[tokio::test]
    async fn segmented_download_matches_source() {
        let total = 3 * 1024 * 1024 + 123;
        let srv = spawn(Cfg { total, ..Cfg::default() }).await;
        let dir = tempfile::tempdir().unwrap();
        let client = Client::new();
        let sp = spec(srv.url.clone(), dir.path(), 4);

        let state = prepare(&client, &sp).await.unwrap();
        assert_eq!(state.segments.len(), 4);
        let res = run(&client, &sp, state, CancellationToken::new(), noop()).await;

        let Outcome::Completed(path) = res.outcome else { panic!("{:?}", res.outcome) };
        assert_eq!(std::fs::read(&path).unwrap(), pattern(total));
        assert!(!res.state.part_path.exists(), ".part dosyası kalmamalı");
        assert_eq!(res.state.downloaded(), total as u64);
    }

    #[tokio::test]
    async fn pause_then_resume_completes_correctly() {
        let total = 4 * 1024 * 1024;
        let srv = spawn(Cfg { total, throttle_ms: 25, ..Cfg::default() }).await;
        let dir = tempfile::tempdir().unwrap();
        let client = Client::new();
        let sp = spec(srv.url.clone(), dir.path(), 4);
        let state = prepare(&client, &sp).await.unwrap();

        // İlk çalıştırma: bir süre sonra duraklat.
        let cancel = CancellationToken::new();
        let c2 = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            c2.cancel();
        });
        let paused = run(&client, &sp, state, cancel, noop()).await;
        assert_eq!(paused.outcome, Outcome::Paused);
        let got = paused.state.downloaded();
        assert!(got > 0 && got < total as u64, "kısmi indirmeli: {got}");

        // Duraklatma anında her parçanın diske yazdığı bayt, kaynakla birebir aynı olmalı.
        let on_disk = std::fs::read(&paused.state.part_path).unwrap();
        let expected = pattern(total);
        for s in &paused.state.segments {
            let (a, b) = (s.start as usize, (s.start + s.downloaded) as usize);
            assert_eq!(on_disk[a..b], expected[a..b], "parça {}-{} bozuk", s.start, s.end);
        }

        // Devam: kalan kısım inip dosya tamamlanmalı.
        let resumed = run(&client, &sp, paused.state, CancellationToken::new(), noop()).await;
        let Outcome::Completed(path) = resumed.outcome else { panic!("{:?}", resumed.outcome) };
        assert_eq!(std::fs::read(&path).unwrap(), expected);
    }

    #[tokio::test]
    async fn falls_back_to_single_connection_without_range_support() {
        let total = 1024 * 1024;
        let srv = spawn(Cfg { total, ranges: false, ..Cfg::default() }).await;
        let dir = tempfile::tempdir().unwrap();
        let client = Client::new();
        let sp = spec(srv.url.clone(), dir.path(), 8);

        let state = prepare(&client, &sp).await.unwrap();
        assert!(!state.accepts_ranges);
        assert_eq!(state.segments.len(), 1);
        let res = run(&client, &sp, state, CancellationToken::new(), noop()).await;
        let Outcome::Completed(path) = res.outcome else { panic!("{:?}", res.outcome) };
        assert_eq!(std::fs::read(&path).unwrap(), pattern(total));
    }

    #[tokio::test]
    async fn retries_transient_server_errors() {
        let total = 2 * 1024 * 1024;
        let srv = spawn(Cfg { total, fail_range_requests: 2, ..Cfg::default() }).await;
        let dir = tempfile::tempdir().unwrap();
        let client = Client::new();
        let sp = spec(srv.url.clone(), dir.path(), 4);

        let state = prepare(&client, &sp).await.unwrap();
        let res = run(&client, &sp, state, CancellationToken::new(), noop()).await;
        let Outcome::Completed(path) = res.outcome else { panic!("{:?}", res.outcome) };
        assert_eq!(std::fs::read(&path).unwrap(), pattern(total));
    }

    #[tokio::test]
    async fn permanent_client_error_fails_without_retrying_forever() {
        let srv = spawn(Cfg { total: 2 * 1024 * 1024, status_override: Some(403), ..Cfg::default() }).await;
        let dir = tempfile::tempdir().unwrap();
        let res = prepare(&Client::new(), &spec(srv.url.clone(), dir.path(), 4)).await;
        assert!(matches!(res, Err(EngineError::Status(403))));
    }

    #[tokio::test]
    async fn unknown_length_downloads_until_stream_ends() {
        let total = 700 * 1024;
        let srv = spawn(Cfg { total, ranges: false, unknown_length: true, ..Cfg::default() }).await;
        let dir = tempfile::tempdir().unwrap();
        let client = Client::new();
        let sp = spec(srv.url.clone(), dir.path(), 4);

        let state = prepare(&client, &sp).await.unwrap();
        assert_eq!(state.total, None);
        let res = run(&client, &sp, state, CancellationToken::new(), noop()).await;
        let Outcome::Completed(path) = res.outcome else { panic!("{:?}", res.outcome) };
        assert_eq!(std::fs::read(&path).unwrap(), pattern(total));
        assert_eq!(res.state.total, Some(total as u64));
    }

    #[tokio::test]
    async fn empty_file_completes() {
        let srv = spawn(Cfg { total: 0, ..Cfg::default() }).await;
        let dir = tempfile::tempdir().unwrap();
        let client = Client::new();
        let sp = spec(srv.url.clone(), dir.path(), 4);
        let state = prepare(&client, &sp).await.unwrap();
        let res = run(&client, &sp, state, CancellationToken::new(), noop()).await;
        let Outcome::Completed(path) = res.outcome else { panic!("{:?}", res.outcome) };
        assert_eq!(std::fs::metadata(path).unwrap().len(), 0);
    }

    #[test]
    fn unique_path_avoids_existing_files_and_part_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.zip"), b"x").unwrap();
        std::fs::write(dir.path().join("a (1).zip.part"), b"x").unwrap();
        assert_eq!(unique_path(dir.path(), "a.zip"), dir.path().join("a (2).zip"));
        assert_eq!(unique_path(dir.path(), "b.zip"), dir.path().join("b.zip"));
    }
}
