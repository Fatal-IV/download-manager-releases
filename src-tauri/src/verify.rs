//! İndirilen dosyanın bütünlük (SHA-256) doğrulaması ve Windows Defender taraması.

use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_bytes(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

/// Dosyayı parça parça okuyup SHA-256 özetini (küçük harfli hex) döndürür. Engelleyicidir.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

/// Kullanıcının yapıştırdığı özeti temizler ("sha256:" öneki, boşluk, büyük harf).
/// Geçerli bir SHA-256 (64 hex hane) değilse `None`.
pub fn normalize_hash(input: &str) -> Option<String> {
    let s = input.trim();
    let s = s.strip_prefix("sha256:").or_else(|| s.strip_prefix("SHA256:")).unwrap_or(s).trim();
    (s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())).then(|| s.to_ascii_lowercase())
}

/// `MpCmdRun.exe` yolunu bulur: önce en yeni platform klasörü, sonra Program Files.
fn find_mpcmdrun() -> Option<PathBuf> {
    if let Some(data) = std::env::var_os("ProgramData") {
        let platform = PathBuf::from(data).join("Microsoft").join("Windows Defender").join("Platform");
        if let Ok(rd) = std::fs::read_dir(platform) {
            let mut dirs: Vec<_> = rd.flatten().map(|e| e.path()).collect();
            dirs.sort();
            for d in dirs.into_iter().rev() {
                let exe = d.join("MpCmdRun.exe");
                if exe.is_file() {
                    return Some(exe);
                }
            }
        }
    }
    let pf = std::env::var_os("ProgramFiles")?;
    let exe = PathBuf::from(pf).join("Windows Defender").join("MpCmdRun.exe");
    exe.is_file().then_some(exe)
}

/// Dosyayı Windows Defender ile tarar. `Ok(true)` temiz, `Ok(false)` tehdit bulundu.
/// Tehdit bulunsa da dosyaya dokunulmaz (`-DisableRemediation`); karar kullanıcıya kalır.
pub async fn defender_scan(path: &Path) -> Result<bool, String> {
    let exe = find_mpcmdrun().ok_or("Windows Defender bulunamadı")?;
    let mut cmd = tokio::process::Command::new(exe);
    cmd.args(["-Scan", "-ScanType", "3", "-DisableRemediation", "-File"]).arg(path);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let out = cmd.output().await.map_err(|e| e.to_string())?;
    interpret_exit(out.status.code())
}

fn interpret_exit(code: Option<i32>) -> Result<bool, String> {
    match code {
        Some(0) => Ok(true),
        Some(2) => Ok(false),
        other => Err(format!("Tarama tamamlanamadı (çıkış kodu {other:?})")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(sha256_bytes(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn file_hash_equals_bytes_hash() {
        let dir = std::env::temp_dir().join(format!("dm-hash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("f.bin");
        let data: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&p, &data).unwrap();
        assert_eq!(sha256_file(&p).unwrap(), sha256_bytes(&data));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn normalize_accepts_only_valid_digests() {
        let h = "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD";
        let want = Some(h.to_ascii_lowercase());
        assert_eq!(normalize_hash(h), want);
        assert_eq!(normalize_hash(&format!("  sha256:{h}\n")), want);
        assert_eq!(normalize_hash("abc"), None);
        assert_eq!(normalize_hash(&"z".repeat(64)), None);
        assert_eq!(normalize_hash(""), None);
    }

    #[test]
    fn defender_exit_codes_map_to_verdicts() {
        assert_eq!(interpret_exit(Some(0)), Ok(true));
        assert_eq!(interpret_exit(Some(2)), Ok(false));
        assert!(interpret_exit(Some(1)).is_err());
        assert!(interpret_exit(None).is_err());
    }
}
