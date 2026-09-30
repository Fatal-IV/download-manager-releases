//! İndirme kayıtlarını, çalışan görevleri ve SQLite kalıcılığını yönetir.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use reqwest::Client;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::category;
use crate::verify;
use crate::video::{self, RunOutcome, RunSpec, Tools};
use crate::engine::{self, filename, JobSpec, JobState, Limiter, Outcome, RunResult, SegMeta};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Queued,
    Downloading,
    Paused,
    Completed,
    Failed,
}

/// İndirmenin nasıl yapıldığı: kendi motorumuz ya da yt-dlp.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Http,
    Video,
}

/// Arayüzün gördüğü biçim (`src/types.ts` ile birebir).
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadDto {
    pub id: String,
    pub url: String,
    pub filename: String,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub speed: f64,
    pub status: Status,
    pub connections: u32,
    pub error: Option<String>,
    pub added_at: u64,
    pub segments: Vec<SegmentDto>,
    /// Dosya türüne göre kategori (Video, Belgeler, ...).
    pub category: &'static str,
    /// Uygulamanın kendisi duraklattıysa nedeni: "network" (ağ yok) ya da "schedule" (zamanlayıcı).
    pub auto_paused: Option<String>,
    /// Hesaplanmış SHA-256 özeti (küçük harfli hex).
    pub sha256: Option<String>,
    /// Kullanıcının verdiği beklenen özetle karşılaştırma sonucu; özet verilmediyse yok.
    pub verified: Option<bool>,
    /// Virüs taraması: "scanning" | "clean" | "threat" | "error".
    pub scan: Option<String>,
    pub kind: Kind,
}

/// Bir parçanın boyutu ve indirilen miktarı.
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SegmentDto {
    pub start: u64,
    pub end: u64,
    pub size: u64,
    pub downloaded: u64,
    /// Sunucuyla yapılan gerçek konuşmanın kaydı (kanıt).
    pub range: String,
    pub http_status: u16,
    pub content_range: Option<String>,
    pub remote: Option<String>,
    pub connections_opened: u32,
    /// Parçanın indirildiği kaynak adres (ana adres ya da yansı).
    pub source: String,
}

/// Kullanıcı ayarları; `settings` tablosunda saklanır.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Yeni indirmeler için varsayılan bağlantı (parça) sayısı.
    pub connections: u32,
    /// Sunucu birden fazla IP'ye çözülüyorsa bağlantıları bunlara dağıt.
    pub spread_ips: bool,
    /// Yeni indirmelerin kaydedileceği klasör; boşsa varsayılan İndirilenler klasörü.
    pub download_dir: String,
    /// Aynı anda çalışabilecek indirme sayısı; fazlası sırada bekler.
    pub max_concurrent: u32,
    /// Tüm indirmeler için toplam hız sınırı (KB/sn); 0 = sınırsız.
    pub speed_limit_kbps: u32,
    /// Açıksa indirmeler yalnızca aşağıdaki saat aralığında çalışır.
    pub schedule_enabled: bool,
    /// Zamanlayıcı başlangıcı, gece yarısından itibaren dakika (0-1439).
    pub schedule_start: u32,
    /// Zamanlayıcı bitişi; başlangıçtan küçükse aralık gece yarısını aşar.
    pub schedule_end: u32,
    /// İndirme bitince Windows bildirimi göster.
    pub notify_on_complete: bool,
    /// Yeni indirmeleri dosya türüne göre alt klasöre (Video, Belgeler, ...) kaydet.
    pub sort_by_category: bool,
    /// İndirme bitince Windows Defender ile tara.
    pub scan_on_complete: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            connections: 8,
            spread_ips: true,
            download_dir: String::new(),
            max_concurrent: 3,
            speed_limit_kbps: 0,
            schedule_enabled: false,
            schedule_start: 2 * 60,
            schedule_end: 8 * 60,
            notify_on_complete: true,
            sort_by_category: false,
            scan_on_complete: false,
        }
    }
}

/// Zamanlayıcı açıksa `minute` (gece yarısından itibaren dakika) izinli aralıkta mı?
fn in_schedule(s: &Settings, minute: u32) -> bool {
    if !s.schedule_enabled || s.schedule_start == s.schedule_end {
        return true;
    }
    if s.schedule_start < s.schedule_end {
        minute >= s.schedule_start && minute < s.schedule_end
    } else {
        minute >= s.schedule_start || minute < s.schedule_end
    }
}

fn local_minute() -> u32 {
    use chrono::Timelike;
    let now = chrono::Local::now();
    now.hour() * 60 + now.minute()
}

/// İnternete çıkılabiliyor mu? Bilinen üç genel adresten birine TCP bağlantısı kurulabilirse evet.
async fn reachable() -> bool {
    use tokio::net::TcpStream;
    let attempts = ["1.1.1.1:443", "8.8.8.8:53", "9.9.9.9:443"].map(|addr| {
        Box::pin(async move {
            match tokio::time::timeout(Duration::from_secs(2), TcpStream::connect(addr)).await {
                Ok(Ok(_)) => Ok(()),
                _ => Err(()),
            }
        })
    });
    futures_util::future::select_ok(attempts).await.is_ok()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Record {
    id: String,
    url: String,
    filename: String,
    connections: u32,
    headers: Vec<(String, String)>,
    dest_dir: PathBuf,
    #[serde(default)]
    mirrors: Vec<String>,
    status: Status,
    error: Option<String>,
    added_at: u64,
    state: Option<JobState>,
    total: u64,
    downloaded: u64,
    /// Uygulama kapanırken (ya da çökerken) sürüyordu; bir sonraki açılışta kendiliğinden sürdürülür.
    #[serde(default)]
    resume_on_start: bool,
    /// Uygulama duraklattıysa nedeni ("network" | "schedule"); koşul düzelince kendiliğinden sürdürülür.
    #[serde(default)]
    auto_paused: Option<String>,
    /// Kullanıcının girdiği beklenen SHA-256.
    #[serde(default)]
    expected_sha256: Option<String>,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    scan: Option<String>,
    #[serde(default)]
    kind: Kind,
    /// Video kalitesi: "best", yükseklik ("720") ya da "audio".
    #[serde(default)]
    quality: String,
    /// yt-dlp'nin yazdığı son dosya (video indirmelerinde `state` yoktur).
    #[serde(default)]
    video_path: Option<PathBuf>,
}

struct Job {
    rec: Record,
    /// Çalışan görev varsa iptal belirteci; yoksa `None`.
    cancel: Option<CancellationToken>,
    speed: f64,
    last_tick: Instant,
    last_bytes: u64,
    last_persist: Instant,
    meta: Vec<SegMeta>,
}

impl Job {
    fn new(rec: Record, cancel: Option<CancellationToken>) -> Self {
        let last_bytes = rec.downloaded;
        Job { rec, cancel, speed: 0.0, last_tick: Instant::now(), last_bytes, last_persist: Instant::now(), meta: Vec::new() }
    }

    fn dto(&self) -> DownloadDto {
        let r = &self.rec;
        DownloadDto {
            id: r.id.clone(),
            url: r.url.clone(),
            filename: r.filename.clone(),
            total_bytes: r.total,
            downloaded_bytes: r.downloaded,
            speed: if r.status == Status::Downloading { self.speed } else { 0.0 },
            status: r.status,
            connections: r.connections,
            error: r.error.clone(),
            added_at: r.added_at,
            segments: match &r.state {
                // Boyutu bilinmeyen indirmede parça yok; yer tutucu gösterilmez.
                Some(s) if s.total.is_some() && s.segments.len() > 1 => s
                    .segments
                    .iter()
                    .enumerate()
                    .map(|(i, g)| {
                        let m = self.meta.get(i).cloned().unwrap_or_default();
                        SegmentDto {
                            start: g.start,
                            end: g.end,
                            size: g.len(),
                            downloaded: g.downloaded.min(g.len()),
                            range: m.range,
                            http_status: m.http_status,
                            content_range: m.content_range,
                            remote: m.remote,
                            connections_opened: m.connections_opened,
                            source: m.source,
                        }
                    })
                    .collect(),
                _ => Vec::new(),
            },
            category: category::of(&r.filename),
            auto_paused: r.auto_paused.clone(),
            sha256: r.sha256.clone(),
            verified: match (&r.expected_sha256, &r.sha256) {
                (Some(want), Some(got)) => Some(want == got),
                _ => None,
            },
            scan: r.scan.clone(),
            kind: r.kind,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    Update(DownloadDto),
    /// İndirme yeni bitti ve kullanıcı bildirim istiyor.
    Completed(DownloadDto),
    Remove(String),
}

type Emitter = Arc<dyn Fn(Event) + Send + Sync>;

struct Inner {
    jobs: Mutex<HashMap<String, Job>>,
    db: Mutex<Connection>,
    client: Client,
    emit: Emitter,
    /// Varsayılan indirme klasörü (ayarda klasör seçilmemişse).
    default_dir: PathBuf,
    settings: Mutex<Settings>,
    sched: Mutex<Sched>,
    /// Boş yer açıldığında ya da ayar değiştiğinde sıradakileri uyandırır.
    slots_changed: Notify,
    /// Tüm indirmelerin paylaştığı hız sınırlayıcı.
    limiter: Arc<Limiter>,
    /// Ağ erişimi var mı; yoksa yeni indirmeler beklemede tutulur.
    online: AtomicBool,
    /// Zamanlayıcının izinli aralığında mıyız?
    in_window: AtomicBool,
    tools: Tools,
}

/// Eşzamanlı indirme sınırı için zamanlayıcı durumu.
#[derive(Default)]
struct Sched {
    running: usize,
    /// Yer bekleyen indirmeler, geliş sırasıyla.
    queue: VecDeque<String>,
}

#[derive(Clone)]
pub struct Manager(Arc<Inner>);

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn rand_suffix() -> u32 {
    use std::hash::{BuildHasher, Hasher};
    std::collections::hash_map::RandomState::new().build_hasher().finish() as u32
}

pub struct AddRequest {
    pub url: String,
    /// Verilmezse ayarlardaki varsayılan kullanılır.
    pub connections: Option<u32>,
    pub headers: Vec<(String, String)>,
    pub filename: Option<String>,
    pub mirrors: Vec<String>,
    /// Bitince karşılaştırılacak SHA-256 (normalleştirilmiş).
    pub expected_sha256: Option<String>,
}

/// Kullanıcı adı/parola (Basic) ve çerez girdilerinden istek başlıkları üretir.
pub fn auth_headers(username: Option<&str>, password: Option<&str>, cookie: Option<&str>) -> Vec<(String, String)> {
    use base64::Engine as _;
    let mut h = Vec::new();
    let user = username.map(str::trim).unwrap_or_default();
    if !user.is_empty() {
        let token = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{}", password.unwrap_or_default()));
        h.push(("Authorization".to_string(), format!("Basic {token}")));
    }
    let cookie = cookie.map(str::trim).unwrap_or_default();
    if !cookie.is_empty() {
        h.push(("Cookie".to_string(), cookie.to_string()));
    }
    h
}

impl Manager {
    pub fn new(db_path: PathBuf, download_dir: PathBuf, emit: Emitter) -> Result<Self, String> {
        let db = Connection::open(&db_path).map_err(|e| e.to_string())?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS downloads (id TEXT PRIMARY KEY, json TEXT NOT NULL);")
            .map_err(|e| e.to_string())?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, json TEXT NOT NULL);")
            .map_err(|e| e.to_string())?;
        let settings: Settings = db
            .query_row("SELECT json FROM settings WHERE key = 'main'", [], |r| r.get::<_, String>(0))
            .ok()
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default();

        let mut jobs = HashMap::new();
        {
            let mut stmt = db.prepare("SELECT json FROM downloads").map_err(|e| e.to_string())?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?;
            for json in rows.flatten() {
                let Ok(mut rec) = serde_json::from_str::<Record>(&json) else { continue };
                // Uygulama kapanırken süren indirmeler duraklatılmış sayılır ve `resume_interrupted` ile sürdürülür.
                if matches!(rec.status, Status::Downloading | Status::Queued) {
                    rec.status = Status::Paused;
                    rec.resume_on_start = true;
                }
                if rec.scan.as_deref() == Some("scanning") {
                    rec.scan = None; // Tarama uygulama kapanınca yarım kalmıştır.
                }
                jobs.insert(rec.id.clone(), Job::new(rec, None));
            }
        }

        let tools = Tools::new(db_path.parent().unwrap_or(std::path::Path::new(".")));
        let limiter = Arc::new(Limiter::new(settings.speed_limit_kbps as u64 * 1024));
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| e.to_string())?;

        Ok(Self(Arc::new(Inner {
            jobs: Mutex::new(jobs),
            db: Mutex::new(db),
            client,
            emit,
            default_dir: download_dir,
            settings: Mutex::new(settings),
            sched: Mutex::new(Sched::default()),
            slots_changed: Notify::new(),
            limiter,
            online: AtomicBool::new(true),
            in_window: AtomicBool::new(true),
            tools,
        })))
    }

    fn persist(&self, rec: &Record) {
        if let Ok(json) = serde_json::to_string(rec) {
            let _ = lock(&self.0.db).execute(
                "INSERT OR REPLACE INTO downloads (id, json) VALUES (?1, ?2)",
                params![rec.id, json],
            );
        }
    }

    pub fn tools(&self) -> Tools {
        self.0.tools.clone()
    }

    pub fn client(&self) -> Client {
        self.0.client.clone()
    }

    /// Bir indirmenin ana adrese gönderdiği başlıklar (çerez vb.); hız testi aynısını kullanır.
    pub fn headers_of(&self, id: &str) -> Vec<(String, String)> {
        lock(&self.0.jobs).get(id).map(|j| j.rec.headers.clone()).unwrap_or_default()
    }

    pub fn settings(&self) -> Settings {
        lock(&self.0.settings).clone()
    }

    pub fn default_download_dir(&self) -> PathBuf {
        self.0.default_dir.clone()
    }

    /// Yeni indirmelerin gideceği klasör: ayardaki klasör, yoksa varsayılan.
    pub fn download_dir(&self) -> PathBuf {
        let dir = lock(&self.0.settings).download_dir.trim().to_string();
        if dir.is_empty() { self.0.default_dir.clone() } else { PathBuf::from(dir) }
    }

    pub fn set_settings(&self, mut s: Settings) -> Settings {
        s.connections = s.connections.clamp(1, 32);
        s.max_concurrent = s.max_concurrent.clamp(1, 32);
        s.speed_limit_kbps = s.speed_limit_kbps.min(10_000_000);
        s.schedule_start = s.schedule_start.min(1439);
        s.schedule_end = s.schedule_end.min(1439);
        s.download_dir = s.download_dir.trim().to_string();
        // Klasör oluşturulamıyorsa (geçersiz yol, izin yok) eski ayar korunur.
        if !s.download_dir.is_empty() && std::fs::create_dir_all(&s.download_dir).is_err() {
            s.download_dir = lock(&self.0.settings).download_dir.clone();
        }
        if let Ok(json) = serde_json::to_string(&s) {
            let _ = lock(&self.0.db).execute(
                "INSERT OR REPLACE INTO settings (key, json) VALUES ('main', ?1)",
                params![json],
            );
        }
        *lock(&self.0.settings) = s.clone();
        self.0.limiter.set_rate(s.speed_limit_kbps as u64 * 1024);
        self.tick_schedule();
        self.0.slots_changed.notify_waiters();
        s
    }

    pub fn list(&self) -> Vec<DownloadDto> {
        let mut v: Vec<_> = lock(&self.0.jobs).values().map(Job::dto).collect();
        v.sort_by_key(|d| d.added_at);
        v
    }

    pub fn add(&self, req: AddRequest) -> Result<DownloadDto, String> {
        let url = req.url.trim().to_string();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err("Yalnızca http/https bağlantıları desteklenir".into());
        }
        let name = req
            .filename
            .as_deref()
            .map(filename::sanitize)
            .or_else(|| filename::from_url(&url))
            .unwrap_or_else(|| "indirme".into());
        let id = format!("{:x}{:x}", now_ms(), rand_suffix());
        let mut dest_dir = self.download_dir();
        if self.settings().sort_by_category {
            dest_dir = dest_dir.join(category::of(&name));
            let _ = std::fs::create_dir_all(&dest_dir);
        }
        let rec = Record {
            id: id.clone(),
            url,
            filename: name,
            connections: req.connections.unwrap_or_else(|| self.settings().connections).clamp(1, 32),
            headers: req.headers,
            dest_dir,
            mirrors: req
                .mirrors
                .into_iter()
                .map(|m| m.trim().to_string())
                .filter(|m| m.starts_with("http://") || m.starts_with("https://"))
                .collect(),
            status: Status::Queued,
            error: None,
            added_at: now_ms(),
            state: None,
            total: 0,
            downloaded: 0,
            resume_on_start: false,
            auto_paused: None,
            expected_sha256: req.expected_sha256,
            sha256: None,
            scan: None,
            kind: Kind::Http,
            quality: String::new(),
            video_path: None,
        };
        Ok(self.start(rec))
    }

    /// Kaydı saklar, kuyruğa alır ve indirmeyi başlatır.
    fn start(&self, rec: Record) -> DownloadDto {
        self.persist(&rec);
        let id = rec.id.clone();
        let token = CancellationToken::new();
        let job = Job::new(rec, Some(token.clone()));
        let dto = job.dto();
        lock(&self.0.jobs).insert(id.clone(), job);
        (self.0.emit)(Event::Update(dto.clone()));
        self.spawn_drive(id, token);
        dto
    }

    /// yt-dlp ile video/ses indirmesi ekler. `title` dosya adı ipucudur; gerçek ad yt-dlp'den gelir.
    pub fn add_video(&self, url: String, quality: String, title: String, headers: Vec<(String, String)>) -> Result<DownloadDto, String> {
        let url = url.trim().to_string();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err("Yalnızca http/https bağlantıları desteklenir".into());
        }
        let name = filename::sanitize(&title);
        let mut dest_dir = self.download_dir();
        if self.settings().sort_by_category {
            dest_dir = dest_dir.join(if quality == "audio" { "Müzik" } else { "Video" });
        }
        let _ = std::fs::create_dir_all(&dest_dir);
        let rec = Record {
            id: format!("{:x}{:x}", now_ms(), rand_suffix()),
            url,
            filename: name,
            connections: 1,
            headers,
            dest_dir,
            mirrors: Vec::new(),
            status: Status::Queued,
            error: None,
            added_at: now_ms(),
            state: None,
            total: 0,
            downloaded: 0,
            resume_on_start: false,
            auto_paused: None,
            expected_sha256: None,
            sha256: None,
            scan: None,
            kind: Kind::Video,
            quality,
            video_path: None,
        };
        Ok(self.start(rec))
    }

    fn spawn_drive(&self, id: String, token: CancellationToken) {
        let me = self.clone();
        tauri::async_runtime::spawn(async move { me.drive(id, token).await });
    }

    pub fn pause(&self, id: &str) {
        if let Some(job) = lock(&self.0.jobs).get_mut(id) {
            job.rec.auto_paused = None;
            if let Some(t) = &job.cancel {
                t.cancel();
            }
        }
    }

    pub fn resume(&self, id: &str) {
        let token = CancellationToken::new();
        let dto = {
            let mut jobs = lock(&self.0.jobs);
            let Some(job) = jobs.get_mut(id) else { return };
            if job.cancel.is_some() || !matches!(job.rec.status, Status::Paused | Status::Failed) {
                return;
            }
            // Kaynak değiştiyse eski parçalar geçersizdir; baştan başla.
            if job.rec.error.as_deref().is_some_and(|e| e.contains("Range isteğini yok saydı")) {
                if let Some(s) = job.rec.state.take() {
                    let _ = std::fs::remove_file(s.part_path);
                }
                job.rec.downloaded = 0;
            }
            job.rec.status = Status::Queued;
            job.rec.resume_on_start = false;
            job.rec.auto_paused = None;
            job.rec.error = None;
            job.cancel = Some(token.clone());
            job.last_tick = Instant::now();
            job.last_bytes = job.rec.downloaded;
            job.speed = 0.0;
            job.dto()
        };
        (self.0.emit)(Event::Update(dto));
        self.spawn_drive(id.to_string(), token);
    }

    pub fn remove(&self, id: &str, delete_file: bool) {
        let Some(job) = lock(&self.0.jobs).remove(id) else { return };
        if let Some(t) = &job.cancel {
            t.cancel();
        }
        let _ = lock(&self.0.db).execute("DELETE FROM downloads WHERE id = ?1", params![id]);
        if let (true, true, Some(p)) = (delete_file, job.rec.status == Status::Completed, &job.rec.video_path) {
            let _ = std::fs::remove_file(p);
        }
        if let Some(s) = &job.rec.state {
            if job.rec.status != Status::Completed {
                let _ = std::fs::remove_file(&s.part_path);
            } else if delete_file {
                let _ = std::fs::remove_file(&s.final_path);
            }
        }
        (self.0.emit)(Event::Remove(id.to_string()));
    }

    /// Tamamlandıysa son dosya, değilse `.part` dosyasının yolu.
    pub fn file_path(&self, id: &str) -> Option<PathBuf> {
        let jobs = lock(&self.0.jobs);
        let job = jobs.get(id)?;
        if let Some(p) = job.rec.video_path.as_ref().filter(|_| job.rec.status == Status::Completed) {
            return Some(p.clone());
        }
        let s = job.rec.state.as_ref()?;
        Some(if job.rec.status == Status::Completed { s.final_path.clone() } else { s.part_path.clone() })
    }

    /// Koşul (ağ yok / zamanlayıcı dışı) yüzünden çalışan ve sıradaki tüm indirmeleri duraklatır.
    fn auto_pause_all(&self, reason: &str) {
        for job in lock(&self.0.jobs).values_mut() {
            if let Some(t) = &job.cancel {
                job.rec.auto_paused = Some(reason.to_string());
                t.cancel();
            }
        }
    }

    /// `reason` yüzünden duraklatılanları sürdürür. Ağ dönünce, ağ hatasıyla düşenler de sürdürülür.
    fn auto_resume(&self, reason: &str) {
        let ids: Vec<String> = lock(&self.0.jobs)
            .values()
            .filter(|j| {
                j.cancel.is_none()
                    && match j.rec.status {
                        Status::Paused => j.rec.auto_paused.as_deref() == Some(reason),
                        Status::Failed => {
                            reason == "network" && j.rec.error.as_deref().is_some_and(|e| e.starts_with("Ağ hatası"))
                        }
                        _ => false,
                    }
            })
            .map(|j| j.rec.id.clone())
            .collect();
        for id in ids {
            self.resume(&id);
        }
    }

    fn set_online(&self, online: bool) {
        if self.0.online.swap(online, Ordering::SeqCst) == online {
            return;
        }
        if online {
            self.auto_resume("network");
        } else {
            self.auto_pause_all("network");
        }
        self.0.slots_changed.notify_waiters();
    }

    /// Zamanlayıcının izinli aralığına girildi/çıkıldıysa indirmeleri sürdürür/duraklatır.
    fn tick_schedule(&self) {
        self.tick_schedule_at(local_minute());
    }

    fn tick_schedule_at(&self, minute: u32) {
        let inside = in_schedule(&self.settings(), minute);
        if self.0.in_window.swap(inside, Ordering::SeqCst) == inside {
            return;
        }
        if inside {
            self.auto_resume("schedule");
        } else {
            self.auto_pause_all("schedule");
        }
        self.0.slots_changed.notify_waiters();
    }

    /// Arka planda ağı ve zamanlayıcıyı izler: ağ kopunca duraklatır, gelince sürdürür.
    pub fn start_monitors(&self) {
        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut offline_streak = 0u32;
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                me.tick_schedule();
                // Veri akıyorsa ağ vardır; yoklamaya gerek yok.
                let flowing = lock(&me.0.jobs).values().any(|j| j.rec.status == Status::Downloading && j.speed > 0.0);
                if flowing || reachable().await {
                    offline_streak = 0;
                    me.set_online(true);
                } else {
                    // Kısa aksamalarda duraklatmamak için art arda iki başarısız yoklama gerekir.
                    offline_streak += 1;
                    if offline_streak >= 2 {
                        me.set_online(false);
                    }
                }
            }
        });
    }

    /// Önceki oturumda kapanma/güncelleme/çökme yüzünden yarım kalan indirmeleri sürdürür.
    pub fn resume_interrupted(&self) {
        let ids: Vec<String> = {
            let jobs = lock(&self.0.jobs);
            let mut v: Vec<_> = jobs
                .values()
                .filter(|j| j.rec.resume_on_start && j.rec.status == Status::Paused && j.cancel.is_none())
                .map(|j| (j.rec.added_at, j.rec.id.clone()))
                .collect();
            v.sort();
            v.into_iter().map(|(_, id)| id).collect()
        };
        for id in ids {
            self.resume(&id);
        }
    }

    /// Uygulama kapanmadan önce tüm indirmeleri duraklatıp durumlarının yazılmasını bekler.
    pub async fn shutdown(&self) {
        for job in lock(&self.0.jobs).values_mut() {
            if let Some(t) = &job.cancel {
                // Kullanıcı duraklatmadı; uygulama kapandığı için duruyor. Açılışta sürdürülecek.
                job.rec.resume_on_start = true;
                t.cancel();
            }
        }
        for _ in 0..60 {
            if lock(&self.0.jobs).values().all(|j| j.cancel.is_none()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    fn progress(&self, id: &str, per_segment: &[u64], meta: &[SegMeta]) {
        let bytes: u64 = per_segment.iter().sum();
        let mut jobs = lock(&self.0.jobs);
        let Some(job) = jobs.get_mut(id) else { return };
        if job.rec.status != Status::Downloading {
            return;
        }
        let dt = job.last_tick.elapsed().as_secs_f64();
        if dt > 0.0 {
            let inst = bytes.saturating_sub(job.last_bytes) as f64 / dt;
            job.speed = if job.speed == 0.0 { inst } else { job.speed * 0.6 + inst * 0.4 };
            job.last_tick = Instant::now();
            job.last_bytes = bytes;
        }
        job.rec.downloaded = bytes;
        job.meta = meta.to_vec();
        if let Some(st) = job.rec.state.as_mut() {
            for (seg, d) in st.segments.iter_mut().zip(per_segment) {
                seg.downloaded = *d;
            }
        }
        // Çökme olursa en fazla birkaç saniyelik ilerleme kaybolsun.
        if job.last_persist.elapsed() >= Duration::from_secs(3) {
            job.last_persist = Instant::now();
            self.persist(&job.rec);
        }
        let dto = job.dto();
        drop(jobs);
        (self.0.emit)(Event::Update(dto));
    }

    /// Boş bir indirme yuvası bekler; sıra geldiyse `true`, beklerken duraklatıldı/silindiyse `false`.
    async fn acquire_slot(&self, id: &str, token: &CancellationToken) -> bool {
        lock(&self.0.sched).queue.push_back(id.to_string());
        loop {
            let notified = self.0.slots_changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let max = self.settings().max_concurrent as usize;
            {
                let mut s = lock(&self.0.sched);
                let pos = s.queue.iter().position(|q| q == id);
                if token.is_cancelled() {
                    s.queue.retain(|q| q != id);
                    drop(s);
                    self.0.slots_changed.notify_waiters();
                    return false;
                }
                if let Some(pos) = pos {
                    let can_run = self.0.online.load(Ordering::SeqCst) && self.0.in_window.load(Ordering::SeqCst);
                    if can_run && s.running < max && pos < max - s.running {
                        s.queue.remove(pos);
                        s.running += 1;
                        return true;
                    }
                }
            }
            tokio::select! {
                _ = &mut notified => {}
                _ = token.cancelled() => {}
            }
        }
    }

    fn release_slot(&self) {
        {
            let mut s = lock(&self.0.sched);
            s.running = s.running.saturating_sub(1);
        }
        self.0.slots_changed.notify_waiters();
    }

    async fn drive(self, id: String, token: CancellationToken) {
        if !self.acquire_slot(&id, &token).await {
            // Sırada beklerken duraklatıldı (ya da silindi).
            let mut jobs = lock(&self.0.jobs);
            if let Some(job) = jobs.get_mut(&id) {
                job.cancel = None;
                job.rec.status = Status::Paused;
                self.persist(&job.rec);
                (self.0.emit)(Event::Update(job.dto()));
            }
            return;
        }
        self.drive_running(id, token).await;
        self.release_slot();
    }

    async fn drive_running(&self, id: String, token: CancellationToken) {
        if lock(&self.0.jobs).get(&id).is_some_and(|j| j.rec.kind == Kind::Video) {
            self.drive_video(id, token).await;
            return;
        }
        let (mut spec, state, hinted_name) = {
            let mut jobs = lock(&self.0.jobs);
            let Some(job) = jobs.get_mut(&id) else { return };
            job.rec.status = Status::Downloading;
            let r = &job.rec;
            (
                JobSpec {
                    url: r.url.clone(),
                    dest_dir: r.dest_dir.clone(),
                    filename: None,
                    connections: r.connections,
                    headers: r.headers.clone(),
                    mirrors: r.mirrors.clone(),
                    spread_ips: self.settings().spread_ips,
                    limiter: Some(self.0.limiter.clone()),
                },
                r.state.clone(),
                r.filename.clone(),
            )
        };

        let state = match state {
            Some(s) => s,
            None => {
                // Eklentiden gelen ad, sunucunun bildirdiğinden önceliklidir.
                spec.filename = Some(hinted_name).filter(|n| n != "indirme");
                match engine::prepare(&self.0.client, &spec).await {
                    Ok(s) => s,
                    Err(e) => {
                        self.finish_failed(&id, e.to_string());
                        return;
                    }
                }
            }
        };

        {
            let mut jobs = lock(&self.0.jobs);
            let Some(job) = jobs.get_mut(&id) else {
                let _ = std::fs::remove_file(&state.part_path);
                return;
            };
            if let Some(n) = state.final_path.file_name() {
                job.rec.filename = n.to_string_lossy().into_owned();
            }
            job.rec.total = state.total.unwrap_or(0);
            job.rec.state = Some(state.clone());
            self.persist(&job.rec);
            (self.0.emit)(Event::Update(job.dto()));
        }

        let me = self.clone();
        let pid = id.clone();
        let result = engine::run(
            &self.0.client,
            &spec,
            state,
            token,
            Arc::new(move |segs: &[u64], meta: &[SegMeta]| me.progress(&pid, segs, meta)),
        )
        .await;
        self.finish(&id, result);
    }

    async fn drive_video(&self, id: String, token: CancellationToken) {
        let (url, dest_dir, quality, headers) = {
            let mut jobs = lock(&self.0.jobs);
            let Some(job) = jobs.get_mut(&id) else { return };
            job.rec.status = Status::Downloading;
            job.last_tick = Instant::now();
            (self.0.emit)(Event::Update(job.dto()));
            let r = &job.rec;
            (r.url.clone(), r.dest_dir.clone(), r.quality.clone(), r.headers.clone())
        };
        if !self.0.tools.ready() {
            self.finish_failed(&id, "Video araçları (yt-dlp, ffmpeg) kurulu değil; videoyu yeniden ekleyin".into());
            return;
        }
        let me = self.clone();
        let pid = id.clone();
        let outcome = video::run(
            RunSpec {
                tools: &self.0.tools,
                url: &url,
                dest_dir: &dest_dir,
                quality: &quality,
                headers: &headers,
                limit_kbps: self.settings().speed_limit_kbps,
            },
            token,
            move |p| me.video_progress(&pid, p),
        )
        .await;
        self.finish_video(&id, outcome);
    }

    fn video_progress(&self, id: &str, p: video::Progress) {
        let mut jobs = lock(&self.0.jobs);
        let Some(job) = jobs.get_mut(id) else { return };
        if job.rec.status != Status::Downloading {
            return;
        }
        job.speed = p.speed;
        job.rec.downloaded = p.downloaded;
        if p.total > 0 {
            job.rec.total = p.total.max(p.downloaded);
        }
        if job.last_persist.elapsed() >= Duration::from_secs(3) {
            job.last_persist = Instant::now();
            self.persist(&job.rec);
        }
        // Arayüzü saniyede en çok birkaç kez güncelle.
        if job.last_tick.elapsed() >= Duration::from_millis(250) {
            job.last_tick = Instant::now();
            (self.0.emit)(Event::Update(job.dto()));
        }
    }

    fn finish_video(&self, id: &str, outcome: RunOutcome) {
        let mut jobs = lock(&self.0.jobs);
        let Some(job) = jobs.get_mut(id) else { return };
        job.cancel = None;
        job.speed = 0.0;
        let mut completed = false;
        match outcome {
            RunOutcome::Done(path) => {
                completed = true;
                job.rec.status = Status::Completed;
                job.rec.error = None;
                if let Some(n) = path.file_name() {
                    job.rec.filename = n.to_string_lossy().into_owned();
                }
                let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(job.rec.total);
                job.rec.total = size;
                job.rec.downloaded = size;
                job.rec.video_path = Some(path);
            }
            RunOutcome::Cancelled => job.rec.status = Status::Paused,
            RunOutcome::Failed(msg) => {
                job.rec.status = Status::Failed;
                job.rec.error = Some(msg);
            }
        }
        self.persist(&job.rec);
        let notify = completed && self.settings().notify_on_complete;
        (self.0.emit)(if notify { Event::Completed(job.dto()) } else { Event::Update(job.dto()) });
        if completed {
            let me = self.clone();
            let id = id.to_string();
            tauri::async_runtime::spawn(async move { me.post_complete(&id).await });
        }
    }

    fn finish_failed(&self, id: &str, msg: String) {
        let mut jobs = lock(&self.0.jobs);
        let Some(job) = jobs.get_mut(id) else { return };
        job.cancel = None;
        job.rec.status = Status::Failed;
        job.rec.error = Some(msg);
        self.persist(&job.rec);
        (self.0.emit)(Event::Update(job.dto()));
    }

    fn finish(&self, id: &str, result: RunResult) {
        let mut jobs = lock(&self.0.jobs);
        let Some(job) = jobs.get_mut(id) else {
            // Görev çalışırken silindi; artığı temizle.
            if !matches!(result.outcome, Outcome::Completed(_)) {
                let _ = std::fs::remove_file(&result.state.part_path);
            }
            return;
        };
        let completed = matches!(&result.outcome, Outcome::Completed(_));
        job.cancel = None;
        job.speed = 0.0;
        job.rec.downloaded = result.state.downloaded();
        if let Some(t) = result.state.total {
            job.rec.total = t;
        }
        match result.outcome {
            Outcome::Completed(path) => {
                job.rec.status = Status::Completed;
                job.rec.error = None;
                job.rec.downloaded = job.rec.total;
                if let Some(n) = path.file_name() {
                    job.rec.filename = n.to_string_lossy().into_owned();
                }
            }
            Outcome::Paused => job.rec.status = Status::Paused,
            Outcome::Failed(msg) => {
                job.rec.status = Status::Failed;
                job.rec.error = Some(msg);
            }
        }
        job.rec.state = Some(result.state);
        self.persist(&job.rec);
        let done = job.rec.status == Status::Completed && self.settings().notify_on_complete;
        (self.0.emit)(if done { Event::Completed(job.dto()) } else { Event::Update(job.dto()) });
        if completed {
            let me = self.clone();
            let id = id.to_string();
            tauri::async_runtime::spawn(async move { me.post_complete(&id).await });
        }
    }

    /// Bitmiş dosya için beklenen özet varsa doğrular; ayar açıksa virüs taraması yapar.
    async fn post_complete(&self, id: &str) {
        let wants_hash = lock(&self.0.jobs).get(id).is_some_and(|j| j.rec.expected_sha256.is_some());
        if wants_hash {
            self.hash_now(id).await;
        }
        if self.settings().scan_on_complete {
            self.scan_now(id).await;
        }
    }

    fn completed_path(&self, id: &str) -> Option<PathBuf> {
        let jobs = lock(&self.0.jobs);
        let job = jobs.get(id).filter(|j| j.rec.status == Status::Completed)?;
        job.rec.video_path.clone().or_else(|| job.rec.state.as_ref().map(|s| s.final_path.clone()))
    }

    fn update_job(&self, id: &str, f: impl FnOnce(&mut Record)) {
        let mut jobs = lock(&self.0.jobs);
        let Some(job) = jobs.get_mut(id) else { return };
        f(&mut job.rec);
        self.persist(&job.rec);
        (self.0.emit)(Event::Update(job.dto()));
    }

    /// SHA-256 özetini arka planda hesaplar.
    pub fn hash(&self, id: &str) {
        let (me, id) = (self.clone(), id.to_string());
        tauri::async_runtime::spawn(async move { me.hash_now(&id).await });
    }

    async fn hash_now(&self, id: &str) {
        let Some(path) = self.completed_path(id) else { return };
        let Ok(Ok(digest)) = tokio::task::spawn_blocking(move || verify::sha256_file(&path)).await else { return };
        self.update_job(id, |r| r.sha256 = Some(digest));
    }

    /// Windows Defender taramasını arka planda başlatır.
    pub fn scan(&self, id: &str) {
        let (me, id) = (self.clone(), id.to_string());
        tauri::async_runtime::spawn(async move { me.scan_now(&id).await });
    }

    async fn scan_now(&self, id: &str) {
        let Some(path) = self.completed_path(id) else { return };
        if lock(&self.0.jobs).get(id).is_some_and(|j| j.rec.scan.as_deref() == Some("scanning")) {
            return;
        }
        self.update_job(id, |r| r.scan = Some("scanning".into()));
        let verdict = match verify::defender_scan(&path).await {
            Ok(true) => ("clean", None),
            Ok(false) => ("threat", None),
            Err(e) => ("error", Some(format!("Virüs taraması: {e}"))),
        };
        self.update_job(id, |r| {
            r.scan = Some(verdict.0.into());
            if verdict.1.is_some() {
                r.error = verdict.1;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::testserver::{spawn, Cfg};

    fn req(url: &str) -> AddRequest {
        AddRequest { url: url.into(), connections: Some(1), headers: Vec::new(), filename: None, mirrors: Vec::new(), expected_sha256: None }
    }

    async fn wait_for(m: &Manager, id: &str, pred: impl Fn(&DownloadDto) -> bool) -> DownloadDto {
        for _ in 0..300 {
            let d = m.list().into_iter().find(|d| d.id == id).unwrap();
            if pred(&d) {
                return d;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("beklenen duruma ulaşılamadı");
    }

    #[test]
    fn auth_headers_build_basic_and_cookie() {
        assert!(auth_headers(None, None, None).is_empty());
        let h = auth_headers(Some("user"), Some("pass"), Some(" sid=1; t=2 "));
        assert_eq!(h[0], ("Authorization".to_string(), "Basic dXNlcjpwYXNz".to_string()));
        assert_eq!(h[1], ("Cookie".to_string(), "sid=1; t=2".to_string()));
        assert_eq!(auth_headers(None, Some("x"), None).len(), 0, "kullanıcı adı yoksa parola tek başına anlamsız");
    }

    #[tokio::test]
    async fn expected_sha256_is_verified_after_download() {
        use crate::engine::testserver::pattern;
        let total = 512 * 1024;
        let srv = spawn(Cfg { total, ..Cfg::default() }).await;
        let good = verify::sha256_bytes(&pattern(total));
        let dir = std::env::temp_dir().join(format!("dm-verify-{}", rand_suffix()));
        std::fs::create_dir_all(&dir).unwrap();
        let m = Manager::new(dir.join("t.db"), dir.clone(), Arc::new(|_| {})).unwrap();

        let ok = m.add(AddRequest { expected_sha256: Some(good.clone()), ..req(&srv.url) }).unwrap();
        let d = wait_for(&m, &ok.id, |d| d.sha256.is_some()).await;
        assert_eq!((d.status, d.verified, d.sha256.as_deref()), (Status::Completed, Some(true), Some(good.as_str())));

        let bad = m.add(AddRequest { expected_sha256: Some("0".repeat(64)), ..req(&format!("{}?b", srv.url)) }).unwrap();
        let d = wait_for(&m, &bad.id, |d| d.sha256.is_some()).await;
        assert_eq!(d.verified, Some(false), "yanlış özet uyuşmazlık olarak işaretlenmeli");

        // Beklenen özet verilmemişse elle hesaplanır; karşılaştırma sonucu yoktur.
        let none = m.add(req(&format!("{}?n", srv.url))).unwrap();
        wait_for(&m, &none.id, |d| d.status == Status::Completed).await;
        m.hash(&none.id);
        let d = wait_for(&m, &none.id, |d| d.sha256.is_some()).await;
        assert_eq!((d.verified, d.sha256.as_deref()), (None, Some(good.as_str())));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn window(start: u32, end: u32) -> Settings {
        Settings { schedule_enabled: true, schedule_start: start, schedule_end: end, ..Settings::default() }
    }

    #[test]
    fn schedule_window_handles_midnight_wraparound() {
        let day = window(9 * 60, 17 * 60);
        assert!(in_schedule(&day, 9 * 60) && in_schedule(&day, 16 * 60 + 59));
        assert!(!in_schedule(&day, 17 * 60) && !in_schedule(&day, 3 * 60));
        let night = window(23 * 60, 7 * 60);
        assert!(in_schedule(&night, 23 * 60 + 30) && in_schedule(&night, 2 * 60) && in_schedule(&night, 6 * 60 + 59));
        assert!(!in_schedule(&night, 7 * 60) && !in_schedule(&night, 12 * 60));
        assert!(in_schedule(&Settings::default(), 12 * 60), "kapalıyken her zaman izinli");
    }

    #[tokio::test]
    async fn speed_limit_slows_download() {
        let srv = spawn(Cfg { total: 1024 * 1024, ..Cfg::default() }).await;
        let dir = std::env::temp_dir().join(format!("dm-limit-{}", rand_suffix()));
        std::fs::create_dir_all(&dir).unwrap();
        let m = Manager::new(dir.join("t.db"), dir.clone(), Arc::new(|_| {})).unwrap();
        m.set_settings(Settings { speed_limit_kbps: 512, ..m.settings() });
        let t = Instant::now();
        m.add(req(&srv.url)).unwrap();
        for _ in 0..200 {
            if count(&m, Status::Completed) == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(count(&m, Status::Completed), 1);
        assert!(t.elapsed() >= Duration::from_millis(1400), "1 MB, 512 KB/sn sınırında ~2 sn sürmeli: {:?}", t.elapsed());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn schedule_pauses_outside_window_and_resumes_inside() {
        let srv = spawn(Cfg { total: 4 * 1024 * 1024, throttle_ms: 20, ..Cfg::default() }).await;
        let dir = std::env::temp_dir().join(format!("dm-schedule-{}", rand_suffix()));
        std::fs::create_dir_all(&dir).unwrap();
        let m = Manager::new(dir.join("t.db"), dir.clone(), Arc::new(|_| {})).unwrap();
        let d = m.add(req(&srv.url)).unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(count(&m, Status::Downloading), 1);

        // Pencere kapandı: kendiliğinden duraklar ve nedeni kaydedilir.
        *lock(&m.0.settings) = window(10 * 60, 11 * 60);
        m.tick_schedule_at(12 * 60);
        tokio::time::sleep(Duration::from_millis(300)).await;
        let cur = m.list().into_iter().find(|x| x.id == d.id).unwrap();
        assert_eq!(cur.status, Status::Paused);
        assert_eq!(cur.auto_paused.as_deref(), Some("schedule"));

        // Pencere açıldı: kendiliğinden sürer ve biter.
        m.tick_schedule_at(10 * 60 + 5);
        for _ in 0..300 {
            if count(&m, Status::Completed) == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(count(&m, Status::Completed), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn network_loss_pauses_and_return_resumes() {
        let srv = spawn(Cfg { total: 4 * 1024 * 1024, throttle_ms: 20, ..Cfg::default() }).await;
        let dir = std::env::temp_dir().join(format!("dm-net-{}", rand_suffix()));
        std::fs::create_dir_all(&dir).unwrap();
        let m = Manager::new(dir.join("t.db"), dir.clone(), Arc::new(|_| {})).unwrap();
        let d = m.add(req(&srv.url)).unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;

        m.set_online(false);
        tokio::time::sleep(Duration::from_millis(300)).await;
        let cur = m.list().into_iter().find(|x| x.id == d.id).unwrap();
        assert_eq!((cur.status, cur.auto_paused.as_deref()), (Status::Paused, Some("network")));

        // Ağ yokken eklenen indirme başlamadan bekler.
        let waiting = m.add(req(&format!("{}?w", srv.url))).unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(m.list().into_iter().find(|x| x.id == waiting.id).unwrap().status, Status::Queued);

        m.set_online(true);
        for _ in 0..400 {
            if count(&m, Status::Completed) == 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(count(&m, Status::Completed), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn count(m: &Manager, st: Status) -> usize {
        m.list().iter().filter(|d| d.status == st).count()
    }

    #[tokio::test]
    async fn downloads_interrupted_by_shutdown_resume_on_next_start() {
        let srv = spawn(Cfg { total: 4 * 1024 * 1024, throttle_ms: 20, ..Cfg::default() }).await;
        let dir = std::env::temp_dir().join(format!("dm-resume-{}", rand_suffix()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("t.db");

        let m = Manager::new(db.clone(), dir.clone(), Arc::new(|_| {})).unwrap();
        let running = m.add(req(&srv.url)).unwrap();
        let paused = m.add(req(&format!("{}?p", srv.url))).unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        m.pause(&paused.id); // Kullanıcı bunu kendisi duraklattı; sürdürülmemeli.
        tokio::time::sleep(Duration::from_millis(200)).await;
        m.shutdown().await;
        drop(m);

        let m2 = Manager::new(db, dir.clone(), Arc::new(|_| {})).unwrap();
        m2.resume_interrupted();
        let status = |m: &Manager, id: &str| m.list().into_iter().find(|d| d.id == id).unwrap().status;
        assert_ne!(status(&m2, &running.id), Status::Paused, "yarım kalan indirme sürdürülmeli");
        assert_eq!(status(&m2, &paused.id), Status::Paused, "kullanıcının duraklattığı sürdürülmemeli");
        for _ in 0..300 {
            if status(&m2, &running.id) == Status::Completed {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(status(&m2, &running.id), Status::Completed);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn concurrent_limit_queues_extra_downloads() {
        let srv = spawn(Cfg { total: 2 * 1024 * 1024, throttle_ms: 20, ..Cfg::default() }).await;
        let dir = std::env::temp_dir().join(format!("dm-sched-{}", rand_suffix()));
        std::fs::create_dir_all(&dir).unwrap();
        let m = Manager::new(dir.join("t.db"), dir.clone(), Arc::new(|_| {})).unwrap();
        m.set_settings(Settings { max_concurrent: 1, ..m.settings() });

        for i in 0..3 {
            m.add(req(&format!("{}?n={i}", srv.url))).unwrap();
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(count(&m, Status::Downloading), 1, "aynı anda yalnızca 1 indirme çalışmalı");
        assert_eq!(count(&m, Status::Queued), 2);

        // Sıradakini duraklat: Paused olmalı, diğerleri etkilenmemeli.
        let queued = m.list().into_iter().rev().find(|d| d.status == Status::Queued).unwrap();
        m.pause(&queued.id);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(count(&m, Status::Paused), 1);

        // Kalan ikisi sırayla biter; hiçbir an 1'den fazla çalışmaz.
        for _ in 0..200 {
            assert!(count(&m, Status::Downloading) <= 1);
            if count(&m, Status::Completed) == 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(count(&m, Status::Completed), 2);

        // Duraklatılan devam ettirilince yuvayı alıp tamamlanır.
        m.resume(&queued.id);
        for _ in 0..200 {
            if count(&m, Status::Completed) == 3 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(count(&m, Status::Completed), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
