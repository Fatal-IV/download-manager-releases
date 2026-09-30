//! yt-dlp ile video/ses indirme. yt-dlp ve ffmpeg ilk kullanımda uygulama veri klasörüne indirilir.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime};

use futures_util::StreamExt;
use reqwest::Client;
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

const YTDLP_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";
const FFMPEG_URL: &str = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";
/// Siteler sık değiştiği için yt-dlp bu süreden eskiyse kullanılmadan önce kendini günceller.
const UPDATE_AFTER: Duration = Duration::from_secs(7 * 24 * 3600);

/// Araçların bulunduğu klasör.
#[derive(Clone)]
pub struct Tools {
    pub dir: PathBuf,
}

impl Tools {
    pub fn new(data_dir: &Path) -> Self {
        Self { dir: data_dir.join("tools") }
    }
    pub fn ytdlp(&self) -> PathBuf {
        self.dir.join("yt-dlp.exe")
    }
    pub fn ffmpeg(&self) -> PathBuf {
        self.dir.join("ffmpeg.exe")
    }
    pub fn ready(&self) -> bool {
        self.ytdlp().is_file() && self.ffmpeg().is_file()
    }
}

/// Konsol penceresi açmadan çalışan bir komut.
fn command(exe: impl AsRef<std::ffi::OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(exe);
    #[cfg(windows)]
    c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    c.kill_on_drop(true);
    c
}

/// URL'yi dosyaya indirir; ilerleme `(inen, toplam)` olarak bildirilir.
async fn download_file(
    client: &Client,
    url: &str,
    dest: &Path,
    on_progress: &(dyn Fn(f64) + Send + Sync),
) -> Result<(), String> {
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Sunucu {} durum kodu döndürdü", resp.status().as_u16()));
    }
    let total = resp.content_length().unwrap_or(0);
    let tmp = dest.with_extension("tmp");
    let mut file = tokio::fs::File::create(&tmp).await.map_err(|e| e.to_string())?;
    let mut stream = resp.bytes_stream();
    let mut done = 0u64;
    let mut last = 0.0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        done += chunk.len() as u64;
        if total > 0 {
            let p = done as f64 / total as f64;
            if p - last >= 0.01 {
                last = p;
                on_progress(p);
            }
        }
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);
    tokio::fs::rename(&tmp, dest).await.map_err(|e| e.to_string())?;
    on_progress(1.0);
    Ok(())
}

/// Eksik araçları indirir. `on_progress(ad, 0..1)` ile ilerleme bildirilir.
pub async fn ensure(client: &Client, tools: &Tools, on_progress: Box<dyn Fn(&str, f64) + Send + Sync>) -> Result<(), String> {
    tokio::fs::create_dir_all(&tools.dir).await.map_err(|e| e.to_string())?;

    if !tools.ytdlp().is_file() {
        let cb = |p: f64| on_progress("yt-dlp", p);
        download_file(client, YTDLP_URL, &tools.ytdlp(), &cb)
            .await
            .map_err(|e| format!("yt-dlp indirilemedi: {e}"))?;
    } else if is_stale(&tools.ytdlp()) {
        // En iyi çaba: güncellenemezse eski sürümle devam edilir.
        let _ = tokio::time::timeout(Duration::from_secs(60), command(tools.ytdlp()).arg("-U").output()).await;
    }

    if !tools.ffmpeg().is_file() {
        let zip = tools.dir.join("ffmpeg.zip");
        let cb = |p: f64| on_progress("ffmpeg", p);
        download_file(client, FFMPEG_URL, &zip, &cb).await.map_err(|e| format!("ffmpeg indirilemedi: {e}"))?;
        let unpacked = tools.dir.join("ffmpeg-unpacked");
        let _ = tokio::fs::remove_dir_all(&unpacked).await;
        tokio::fs::create_dir_all(&unpacked).await.map_err(|e| e.to_string())?;
        // Windows 10+ ile gelen tar.exe zip dosyalarını da açar.
        let out = command("tar").arg("-xf").arg(&zip).arg("-C").arg(&unpacked).output().await.map_err(|e| e.to_string())?;
        let exe = find_file(&unpacked, "ffmpeg.exe");
        let result = match (out.status.success(), exe) {
            (true, Some(exe)) => tokio::fs::rename(&exe, tools.ffmpeg()).await.map_err(|e| e.to_string()),
            _ => Err("ffmpeg arşivi açılamadı".to_string()),
        };
        let _ = tokio::fs::remove_dir_all(&unpacked).await;
        let _ = tokio::fs::remove_file(&zip).await;
        result?;
    }
    Ok(())
}

fn is_stale(path: &Path) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age > UPDATE_AFTER)
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if let Some(found) = find_file(&p, name) {
                return Some(found);
            }
        } else if p.file_name().is_some_and(|n| n.eq_ignore_ascii_case(name)) {
            return Some(p);
        }
    }
    None
}

/// Arayüze dönen video özeti.
#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    pub title: String,
    pub uploader: Option<String>,
    /// Saniye.
    pub duration: Option<f64>,
    pub thumbnail: Option<String>,
    /// Sunulan görüntü yükseklikleri, büyükten küçüğe (ör. 1080, 720, 480).
    pub heights: Vec<u32>,
}

pub fn parse_info(json: &str) -> Result<VideoInfo, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("Yanıt çözümlenemedi: {e}"))?;
    let title = v["title"].as_str().filter(|t| !t.is_empty()).ok_or("Videonun başlığı alınamadı")?.to_string();
    let mut heights: Vec<u32> = v["formats"]
        .as_array()
        .map(|fs| {
            fs.iter()
                .filter(|f| f["vcodec"].as_str().is_some_and(|c| c != "none"))
                .filter_map(|f| f["height"].as_u64())
                .map(|h| h as u32)
                .collect()
        })
        .unwrap_or_default();
    heights.sort_unstable_by(|a, b| b.cmp(a));
    heights.dedup();
    Ok(VideoInfo {
        title,
        uploader: v["uploader"].as_str().map(str::to_string),
        duration: v["duration"].as_f64(),
        thumbnail: v["thumbnail"].as_str().map(str::to_string),
        heights,
    })
}

fn header_args(headers: &[(String, String)]) -> Vec<String> {
    headers.iter().flat_map(|(k, v)| ["--add-header".to_string(), format!("{k}:{v}")]).collect()
}

/// Videonun başlığını ve sunulan kalitelerini alır.
pub async fn info(tools: &Tools, url: &str, headers: &[(String, String)]) -> Result<VideoInfo, String> {
    let out = command(tools.ytdlp())
        .args(["-J", "--no-playlist", "--no-warnings"])
        .args(header_args(headers))
        .arg("--")
        .arg(url)
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(last_error(&String::from_utf8_lossy(&out.stderr)));
    }
    parse_info(&String::from_utf8_lossy(&out.stdout))
}

/// `quality`: "best", bir yükseklik ("1080", "720", ...) ya da "audio".
pub fn format_args(quality: &str) -> Vec<String> {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect();
    match quality {
        "audio" => s(&["-f", "ba/b", "-x", "--audio-format", "mp3"]),
        h if h.parse::<u32>().is_ok() => {
            vec!["-f".into(), format!("bv*[height<={h}]+ba/b[height<={h}]/b"), "--merge-output-format".into(), "mp4".into()]
        }
        _ => s(&["-f", "bv*+ba/b", "--merge-output-format", "mp4"]),
    }
}

/// yt-dlp'nin ilerleme satırı: `DMP|inen|toplam|tahmini toplam|hız`.
#[derive(Debug, PartialEq)]
pub struct Progress {
    pub downloaded: u64,
    pub total: u64,
    pub speed: f64,
}

pub fn parse_progress(line: &str) -> Option<Progress> {
    let rest = line.trim().strip_prefix("DMP|")?;
    let f: Vec<&str> = rest.split('|').collect();
    if f.len() < 4 {
        return None;
    }
    let num = |s: &str| s.trim().parse::<f64>().ok();
    Some(Progress {
        downloaded: num(f[0])? as u64,
        total: num(f[1]).or_else(|| num(f[2])).unwrap_or(0.0) as u64,
        speed: num(f[3]).unwrap_or(0.0),
    })
}

/// stderr'den kullanıcıya gösterilecek son "ERROR:" satırını çıkarır.
fn last_error(stderr: &str) -> String {
    stderr
        .lines()
        .rev()
        .find(|l| l.contains("ERROR:"))
        .or_else(|| stderr.lines().rev().find(|l| !l.trim().is_empty()))
        .map(|l| l.trim().trim_start_matches("ERROR:").trim().to_string())
        .unwrap_or_else(|| "yt-dlp başarısız oldu".to_string())
}

pub enum RunOutcome {
    Done(PathBuf),
    Cancelled,
    Failed(String),
}

pub struct RunSpec<'a> {
    pub tools: &'a Tools,
    pub url: &'a str,
    pub dest_dir: &'a Path,
    pub quality: &'a str,
    pub headers: &'a [(String, String)],
    /// KB/sn; 0 = sınırsız.
    pub limit_kbps: u32,
}

/// Videoyu indirir. İptalde süreç ağacı öldürülür; yt-dlp bir sonraki çalıştırmada `.part` dosyasından sürdürür.
pub async fn run(spec: RunSpec<'_>, cancel: CancellationToken, on_progress: impl Fn(Progress)) -> RunOutcome {
    let mut cmd = command(spec.tools.ytdlp());
    cmd.args(["--no-playlist", "--no-warnings", "--newline", "--no-colors", "--progress", "--no-simulate"])
        .args(["--progress-template", "download:DMP|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s"])
        .args(["--print", "after_move:DMF|%(filepath)s"])
        .args(["-N", "4", "--ffmpeg-location"])
        .arg(&spec.tools.dir)
        .arg("-P")
        .arg(spec.dest_dir)
        .args(["-o", "%(title).150B [%(id)s].%(ext)s"])
        .args(format_args(spec.quality))
        .args(header_args(spec.headers));
    if spec.limit_kbps > 0 {
        cmd.arg("--limit-rate").arg(format!("{}K", spec.limit_kbps));
    }
    cmd.arg("--").arg(spec.url).stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return RunOutcome::Failed(format!("yt-dlp başlatılamadı: {e}")),
    };
    let pid = child.id();
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout")).lines();
    let stderr = child.stderr.take().expect("stderr");
    let err_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut all = String::new();
        while let Ok(Some(l)) = lines.next_line().await {
            all.push_str(&l);
            all.push('\n');
        }
        all
    });

    let mut final_path: Option<PathBuf> = None;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                kill_tree(pid).await;
                let _ = child.wait().await;
                return RunOutcome::Cancelled;
            }
            line = stdout.next_line() => match line {
                Ok(Some(l)) => {
                    if let Some(p) = parse_progress(&l) {
                        on_progress(p);
                    } else if let Some(path) = l.trim().strip_prefix("DMF|") {
                        final_path = Some(PathBuf::from(path));
                    }
                }
                _ => break,
            }
        }
    }
    let status = child.wait().await;
    let stderr = err_task.await.unwrap_or_default();
    match (status, final_path) {
        (Ok(s), Some(p)) if s.success() => RunOutcome::Done(p),
        (Ok(s), None) if s.success() => RunOutcome::Failed("yt-dlp dosya yolunu bildirmedi".into()),
        _ => RunOutcome::Failed(last_error(&stderr)),
    }
}

/// yt-dlp.exe (PyInstaller) bir alt süreç başlatır; ikisini birden sonlandırmak için ağacı öldürürüz.
async fn kill_tree(pid: Option<u32>) {
    if let Some(pid) = pid {
        let _ = command("taskkill").args(["/T", "/F", "/PID", &pid.to_string()]).output().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_progress_lines() {
        assert_eq!(
            parse_progress("DMP|1024|4096|NA|512.5"),
            Some(Progress { downloaded: 1024, total: 4096, speed: 512.5 })
        );
        // Kesin toplam yoksa tahmini toplam kullanılır; hız bilinmiyorsa 0.
        assert_eq!(parse_progress("DMP|10|NA|2000|NA"), Some(Progress { downloaded: 10, total: 2000, speed: 0.0 }));
        assert_eq!(parse_progress("[download] 50%"), None);
        assert_eq!(parse_progress("DMP|NA|1|1|1"), None);
    }

    #[test]
    fn parses_video_info_and_heights() {
        let json = r#"{"title":"Me at the zoo","uploader":"jawed","duration":19.0,"thumbnail":"http://x/t.jpg",
            "formats":[{"vcodec":"avc1","height":144},{"vcodec":"avc1","height":720},{"vcodec":"none","height":null},
                       {"vcodec":"vp9","height":720},{"vcodec":"avc1","height":360}]}"#;
        let i = parse_info(json).unwrap();
        assert_eq!(i.title, "Me at the zoo");
        assert_eq!(i.heights, vec![720, 360, 144]);
        assert!(parse_info("{}").is_err());
        assert!(parse_info("bozuk").is_err());
    }

    #[test]
    fn quality_maps_to_format_selectors() {
        assert!(format_args("best").contains(&"bv*+ba/b".to_string()));
        assert!(format_args("720").contains(&"bv*[height<=720]+ba/b[height<=720]/b".to_string()));
        let audio = format_args("audio");
        assert!(audio.contains(&"-x".to_string()) && audio.contains(&"mp3".to_string()));
    }

    #[test]
    fn extracts_last_error_line() {
        let err = "WARNING: x\nERROR: [youtube] abc: Video unavailable\n";
        assert_eq!(last_error(err), "[youtube] abc: Video unavailable");
        assert_eq!(last_error(""), "yt-dlp başarısız oldu");
    }

    /// Gerçek ağ ve yt-dlp gerektirir: `cargo test --lib real_video -- --ignored`
    #[tokio::test]
    #[ignore]
    async fn real_video_download() {
        let dir = std::env::temp_dir().join("dm-video-real");
        let _ = std::fs::remove_dir_all(&dir);
        let tools = Tools::new(&dir);
        let client = Client::new();
        ensure(&client, &tools, Box::new(|_, _| {})).await.unwrap();
        let url = "https://www.youtube.com/watch?v=jNQXAC9IVRw";
        let info = info(&tools, url, &[]).await.unwrap();
        assert_eq!(info.title, "Me at the zoo");
        let out = run(
            RunSpec { tools: &tools, url, dest_dir: &dir.join("out"), quality: "360", headers: &[], limit_kbps: 0 },
            CancellationToken::new(),
            |_| {},
        )
        .await;
        match out {
            RunOutcome::Done(p) => assert!(std::fs::metadata(p).unwrap().len() > 10_000),
            RunOutcome::Failed(e) => panic!("{e}"),
            RunOutcome::Cancelled => panic!("iptal edildi"),
        }
    }
}
