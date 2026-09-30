use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, State};
use tokio_util::sync::CancellationToken;

use crate::manager::{AddRequest, DownloadDto, Manager, Settings};
use crate::speedtest;

/// Çalışan hız testinin iptal belirteci; aynı anda tek test çalışır.
#[derive(Default)]
pub struct SpeedTestState(pub Mutex<Option<CancellationToken>>);

#[tauri::command]
pub fn list_downloads(m: State<'_, Manager>) -> Vec<DownloadDto> {
    m.list()
}

#[tauri::command]
pub fn add_download(
    m: State<'_, Manager>,
    url: String,
    connections: Option<u32>,
    mirrors: Option<Vec<String>>,
    sha256: Option<String>,
    username: Option<String>,
    password: Option<String>,
    cookie: Option<String>,
) -> Result<DownloadDto, String> {
    let expected_sha256 = match sha256.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => Some(crate::verify::normalize_hash(raw).ok_or("SHA-256 değeri geçersiz: 64 haneli onaltılık bir özet olmalı")?),
        None => None,
    };
    m.add(AddRequest {
        url,
        connections,
        headers: crate::manager::auth_headers(username.as_deref(), password.as_deref(), cookie.as_deref()),
        filename: None,
        mirrors: mirrors.unwrap_or_default(),
        expected_sha256,
    })
}

/// Video araçlarını (yt-dlp, ffmpeg) gerekirse indirir ve videonun başlığını/kalitelerini döndürür.
/// Araç indirme ilerlemesi `tools-progress` olayıyla gelir.
#[tauri::command]
pub async fn video_prepare(
    app: AppHandle,
    m: State<'_, Manager>,
    url: String,
    username: Option<String>,
    password: Option<String>,
    cookie: Option<String>,
) -> Result<crate::video::VideoInfo, String> {
    let tools = m.tools();
    let emitter = app.clone();
    crate::video::ensure(
        &m.client(),
        &tools,
        Box::new(move |what, progress| {
            let _ = emitter.emit("tools-progress", serde_json::json!({ "what": what, "progress": progress }));
        }),
    )
    .await?;
    let headers = crate::manager::auth_headers(username.as_deref(), password.as_deref(), cookie.as_deref());
    crate::video::info(&tools, url.trim(), &headers).await
}

/// Videoyu yt-dlp ile indirmek üzere kuyruğa ekler.
#[tauri::command]
pub fn add_video(
    m: State<'_, Manager>,
    url: String,
    quality: String,
    title: String,
    username: Option<String>,
    password: Option<String>,
    cookie: Option<String>,
) -> Result<DownloadDto, String> {
    let headers = crate::manager::auth_headers(username.as_deref(), password.as_deref(), cookie.as_deref());
    m.add_video(url, quality, title, headers)
}

/// Tamamlanan dosyanın SHA-256 özetini hesaplar ve kaydeder; sonuç `download-update` olayıyla gelir.
#[tauri::command]
pub fn hash_download(m: State<'_, Manager>, id: String) {
    m.hash(&id);
}

/// Tamamlanan dosyayı Windows Defender ile tarar; sonuç `download-update` olayıyla gelir.
#[tauri::command]
pub fn scan_download(m: State<'_, Manager>, id: String) {
    m.scan(&id);
}

#[tauri::command]
pub fn get_settings(m: State<'_, Manager>) -> Settings {
    m.settings()
}

#[tauri::command]
pub fn set_settings(m: State<'_, Manager>, settings: Settings) -> Settings {
    m.set_settings(settings)
}

#[tauri::command]
pub fn default_download_dir(m: State<'_, Manager>) -> String {
    m.default_download_dir().to_string_lossy().into_owned()
}

/// Klasör seçme penceresini açar; iptal edilirse `None` döner.
#[tauri::command]
pub async fn pick_folder(app: tauri::AppHandle, start: Option<String>) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let mut dialog = app.dialog().file();
    if let Some(dir) = start.filter(|d| std::path::Path::new(d).is_dir()) {
        dialog = dialog.set_directory(dir);
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    dialog.pick_folder(move |p| {
        let _ = tx.send(p);
    });
    rx.await.ok().flatten().map(|p| p.to_string())
}

/// Güncelleme kurulmadan önce süren indirmeleri duraklatıp durumlarını diske yazar.
#[tauri::command]
pub async fn prepare_for_update(m: State<'_, Manager>) -> Result<(), ()> {
    m.shutdown().await;
    Ok(())
}

#[tauri::command]
pub fn pause_download(m: State<'_, Manager>, id: String) {
    m.pause(&id);
}

#[tauri::command]
pub fn resume_download(m: State<'_, Manager>, id: String) {
    m.resume(&id);
}

#[tauri::command]
pub fn remove_download(m: State<'_, Manager>, id: String, delete_file: bool) {
    m.remove(&id, delete_file);
}

/// Dosyayı varsayılan uygulamayla açar.
#[tauri::command]
pub fn open_download(m: State<'_, Manager>, id: String) -> Result<(), String> {
    let p = m.file_path(&id).ok_or("Dosya bulunamadı")?;
    std::process::Command::new("cmd")
        .args(["/C", "start", ""])
        .arg(p)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Dosyayı Gezgin'de seçili gösterir.
#[tauri::command]
pub fn reveal_download(m: State<'_, Manager>, id: String) -> Result<(), String> {
    let p = m.file_path(&id).ok_or("Dosya bulunamadı")?;
    std::process::Command::new("explorer")
        .arg(format!("/select,{}", p.display()))
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Bağlantı sayısı hız testi. Canlı sonuçlar `speedtest-update` olayıyla gelir, nihai liste döner.
#[tauri::command]
pub async fn speed_test(
    app: AppHandle,
    m: State<'_, Manager>,
    st: State<'_, SpeedTestState>,
    url: String,
    id: Option<String>,
) -> Result<Vec<speedtest::Update>, String> {
    let token = CancellationToken::new();
    {
        let mut slot = st.0.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_some() {
            return Err("Zaten bir hız testi çalışıyor".into());
        }
        *slot = Some(token.clone());
    }
    let headers = id.map(|i| m.headers_of(&i)).unwrap_or_default();
    let emitter = app.clone();
    let emit: Arc<dyn Fn(speedtest::Update) + Send + Sync> = Arc::new(move |u| {
        let _ = emitter.emit("speedtest-update", u);
    });
    let result =
        speedtest::run(&m.client(), &url, &headers, &speedtest::Config::default(), token, emit).await;
    *st.0.lock().unwrap_or_else(|e| e.into_inner()) = None;
    result
}

#[tauri::command]
pub fn cancel_speed_test(st: State<'_, SpeedTestState>) {
    if let Some(t) = st.0.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        t.cancel();
    }
}
