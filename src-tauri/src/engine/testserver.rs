//! Motor testleri için yerel HTTP sunucusu. Gerçek ağa çıkmadan Range, yavaş bağlantı,
//! geçici hata ve boyutsuz (chunked) yanıt senaryolarını üretir.

use std::convert::Infallible;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use futures_util::stream;

/// Kaynağı tam olarak yeniden üretebilmek için deterministik ama düzensiz bayt deseni.
pub fn pattern(len: usize) -> Vec<u8> {
    (0..len).map(|i| ((i * 31) ^ (i >> 8)) as u8).collect()
}

#[derive(Clone)]
pub struct Cfg {
    pub total: usize,
    /// `false` ise sunucu Range başlığını yok sayıp her zaman 200 ile tamamını gönderir.
    pub ranges: bool,
    /// Her 64 KiB'lık parça arasında beklenecek süre (yavaş bağlantı simülasyonu).
    pub throttle_ms: u64,
    /// `start > 0` olan ilk N Range isteğine 503 döner (probe etkilenmez).
    pub fail_range_requests: usize,
    /// Tüm isteklere bu durum kodunu döner.
    pub status_override: Option<u16>,
    /// `true` ise `Content-Length` gönderilmez.
    pub unknown_length: bool,
    /// 0 değilse bu kadar eşzamanlı indirme bağlantısından fazlasına 429 + `Retry-After: 1` döner.
    pub max_concurrent: usize,
}

impl Default for Cfg {
    fn default() -> Self {
        Self {
            total: 1024 * 1024,
            ranges: true,
            throttle_ms: 0,
            fail_range_requests: 0,
            status_override: None,
            unknown_length: false,
            max_concurrent: 0,
        }
    }
}

struct AppState {
    cfg: Cfg,
    data: Vec<u8>,
    failed: AtomicUsize,
    active: AtomicUsize,
    /// Tüm sunucu ömrü boyunca görülen en yüksek eşzamanlı bağlantı sayısı.
    peak: AtomicUsize,
}

/// Açık bir indirme bağlantısını sayar; düşünce sayaç azalır.
struct ConnGuard(Arc<AppState>);

impl Drop for ConnGuard {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
    }
}

pub struct TestServer {
    pub url: String,
    state: Arc<AppState>,
}

impl TestServer {
    pub fn peak_concurrent(&self) -> usize {
        self.state.peak.load(Ordering::SeqCst)
    }
}

pub async fn spawn(cfg: Cfg) -> TestServer {
    let state = Arc::new(AppState { data: pattern(cfg.total), cfg, failed: AtomicUsize::new(0), active: AtomicUsize::new(0), peak: AtomicUsize::new(0) });
    let shared = state.clone();
    let app = Router::new().route("/file", get(handler)).with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    TestServer { url: format!("http://{addr}/file"), state: shared }
}

/// `bytes=a-b` / `bytes=a-` biçimini ayrıştırır.
fn parse_range(h: &HeaderMap, total: usize) -> Option<(usize, usize)> {
    let v = h.get(header::RANGE)?.to_str().ok()?.strip_prefix("bytes=")?;
    let (a, b) = v.split_once('-')?;
    let start: usize = a.parse().ok()?;
    let end = if b.is_empty() { total - 1 } else { b.parse::<usize>().ok()?.min(total - 1) };
    (start <= end).then_some((start, end))
}

async fn handler(State(st): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let cfg = &st.cfg;
    if let Some(code) = cfg.status_override {
        return Response::builder().status(code).body(Body::empty()).unwrap();
    }

    let range = if cfg.ranges && cfg.total > 0 { parse_range(&headers, cfg.total) } else { None };
    if let Some((start, _)) = range {
        // Probe (bytes=0-0) sayılmaz; yalnızca gerçek parça istekleri başarısız olur.
        if start > 0 && st.failed.load(Ordering::SeqCst) < cfg.fail_range_requests {
            st.failed.fetch_add(1, Ordering::SeqCst);
            return Response::builder().status(StatusCode::SERVICE_UNAVAILABLE).body(Body::empty()).unwrap();
        }
    }

    // Probe (1 baytlık) hariç gerçek indirme bağlantılarını say ve sınırla.
    let counted = range.is_none_or(|(s, e)| e > s);
    let guard = if counted {
        let now = st.active.fetch_add(1, Ordering::SeqCst) + 1;
        let guard = ConnGuard(st.clone());
        if cfg.max_concurrent > 0 && now > cfg.max_concurrent {
            return Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .header(header::RETRY_AFTER, "1")
                .body(Body::empty())
                .unwrap();
        }
        st.peak.fetch_max(now, Ordering::SeqCst);
        Some(guard)
    } else {
        None
    };

    let (status, start, end) = match range {
        Some((s, e)) => (StatusCode::PARTIAL_CONTENT, s, e),
        None => (StatusCode::OK, 0, cfg.total.saturating_sub(1)),
    };
    let len = if cfg.total == 0 { 0 } else { end - start + 1 };

    let mut b = Response::builder()
        .status(status)
        .header(header::CONTENT_DISPOSITION, r#"attachment; filename="test file.bin""#)
        .header(header::ETAG, "\"v1\"");
    if !cfg.unknown_length {
        b = b.header(header::CONTENT_LENGTH, len);
    }
    if cfg.ranges {
        b = b.header(header::ACCEPT_RANGES, "bytes");
    }
    if status == StatusCode::PARTIAL_CONTENT {
        b = b.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{}", cfg.total));
    }

    let st2 = st.clone();
    let body = stream::unfold(start, move |pos| {
        let st = st2.clone();
        let _keep = &guard;
        async move {
            if len == 0 || pos > end {
                return None;
            }
            if st.cfg.throttle_ms > 0 {
                tokio::time::sleep(Duration::from_millis(st.cfg.throttle_ms)).await;
            }
            let stop = (pos + 64 * 1024).min(end + 1);
            let chunk = Bytes::copy_from_slice(&st.data[pos..stop]);
            Some((Ok::<_, Infallible>(chunk), stop))
        }
    });
    b.body(Body::from_stream(body)).unwrap()
}
