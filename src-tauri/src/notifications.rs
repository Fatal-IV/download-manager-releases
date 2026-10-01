//! Uygulama içi bildirimler: türler, ayar grupları, metinler ve SQLite'ta kalıcı geçmiş.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Geçmişte tutulan en çok bildirim sayısı.
pub const MAX_KEPT: i64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    DownloadComplete,
    DownloadFailed,
    Hash,
    VerifyOk,
    VerifyFail,
    ScanClean,
    ScanError,
    ScanThreat,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::DownloadComplete => "download_complete",
            Kind::DownloadFailed => "download_failed",
            Kind::Hash => "hash",
            Kind::VerifyOk => "verify_ok",
            Kind::VerifyFail => "verify_fail",
            Kind::ScanClean => "scan_clean",
            Kind::ScanError => "scan_error",
            Kind::ScanThreat => "scan_threat",
        }
    }

    pub fn level(self) -> &'static str {
        match self {
            Kind::Hash | Kind::ScanError => "info",
            Kind::DownloadComplete | Kind::VerifyOk | Kind::ScanClean => "success",
            Kind::DownloadFailed | Kind::VerifyFail | Kind::ScanThreat => "danger",
        }
    }

    /// Ayarlarla kapatılamayan güvenlik uyarıları.
    pub fn is_critical(self) -> bool {
        matches!(self, Kind::VerifyFail | Kind::ScanThreat)
    }
}

/// Hangi bildirim gruplarının açık olduğu; hepsi varsayılan açık.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NotifyKinds {
    pub download_complete: bool,
    pub download_failed: bool,
    pub hash: bool,
    pub verify: bool,
    pub scan: bool,
}

impl Default for NotifyKinds {
    fn default() -> Self {
        Self { download_complete: true, download_failed: true, hash: true, verify: true, scan: true }
    }
}

impl NotifyKinds {
    pub fn allows(&self, kind: Kind) -> bool {
        if kind.is_critical() {
            return true;
        }
        match kind {
            Kind::DownloadComplete => self.download_complete,
            Kind::DownloadFailed => self.download_failed,
            Kind::Hash => self.hash,
            Kind::VerifyOk | Kind::VerifyFail => self.verify,
            Kind::ScanClean | Kind::ScanError | Kind::ScanThreat => self.scan,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationDto {
    pub id: i64,
    pub kind: String,
    pub level: String,
    pub title: String,
    pub body: String,
    pub download_id: Option<String>,
    pub created_at: i64,
    pub read: bool,
}

/// Bildirimin başlığı ve gövdesi.
pub fn compose(kind: Kind, filename: &str, detail: Option<&str>) -> (String, String) {
    let with_detail = |d: Option<&str>| match d {
        Some(d) if !d.is_empty() => format!("{filename}: {d}"),
        _ => filename.to_string(),
    };
    let (title, body) = match kind {
        Kind::DownloadComplete => ("İndirme tamamlandı", filename.to_string()),
        Kind::DownloadFailed => ("İndirme başarısız", with_detail(detail)),
        Kind::Hash => ("SHA-256 hesaplandı", format!("{filename}\n{}", detail.unwrap_or_default())),
        Kind::VerifyOk => ("Özet doğrulandı", filename.to_string()),
        Kind::VerifyFail => ("Özet uyuşmuyor", filename.to_string()),
        Kind::ScanClean => ("Virüs taraması temiz", filename.to_string()),
        Kind::ScanError => ("Virüs taraması yapılamadı", with_detail(detail)),
        Kind::ScanThreat => ("Tehdit bulundu", format!("{filename} — Dosyayı açma.")),
    };
    (title.to_string(), body)
}

pub fn init(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS notifications (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            kind TEXT NOT NULL,
            level TEXT NOT NULL,
            title TEXT NOT NULL,
            body TEXT NOT NULL,
            download_id TEXT,
            created_at INTEGER NOT NULL,
            read INTEGER NOT NULL DEFAULT 0
        );",
    )
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// Ekler, sonra `MAX_KEPT` ötesindeki eski kayıtları siler.
pub fn insert(
    db: &Connection,
    kind: Kind,
    title: &str,
    body: &str,
    download_id: Option<&str>,
) -> rusqlite::Result<NotificationDto> {
    let created_at = now_ms();
    db.execute(
        "INSERT INTO notifications (kind, level, title, body, download_id, created_at, read) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
        params![kind.as_str(), kind.level(), title, body, download_id, created_at],
    )?;
    let id = db.last_insert_rowid();
    db.execute(
        "DELETE FROM notifications WHERE id NOT IN (SELECT id FROM notifications ORDER BY id DESC LIMIT ?1)",
        params![MAX_KEPT],
    )?;
    Ok(NotificationDto {
        id,
        kind: kind.as_str().into(),
        level: kind.level().into(),
        title: title.into(),
        body: body.into(),
        download_id: download_id.map(str::to_owned),
        created_at,
        read: false,
    })
}

/// En yeni üstte.
pub fn list(db: &Connection) -> Vec<NotificationDto> {
    let Ok(mut stmt) = db.prepare(
        "SELECT id, kind, level, title, body, download_id, created_at, read FROM notifications ORDER BY id DESC",
    ) else {
        return Vec::new();
    };
    let rows = stmt.query_map([], |r| {
        Ok(NotificationDto {
            id: r.get(0)?,
            kind: r.get(1)?,
            level: r.get(2)?,
            title: r.get(3)?,
            body: r.get(4)?,
            download_id: r.get(5)?,
            created_at: r.get(6)?,
            read: r.get::<_, i64>(7)? != 0,
        })
    });
    match rows {
        Ok(rows) => rows.flatten().collect(),
        Err(_) => Vec::new(),
    }
}

pub fn mark_read(db: &Connection, id: i64) {
    let _ = db.execute("UPDATE notifications SET read = 1 WHERE id = ?1", params![id]);
}

pub fn mark_all_read(db: &Connection) {
    let _ = db.execute("UPDATE notifications SET read = 1", []);
}

pub fn delete(db: &Connection, id: i64) {
    let _ = db.execute("DELETE FROM notifications WHERE id = ?1", params![id]);
}

pub fn clear(db: &Connection) {
    let _ = db.execute("DELETE FROM notifications", []);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        init(&db).unwrap();
        db
    }

    fn add(db: &Connection, n: usize) -> NotificationDto {
        insert(db, Kind::Hash, &format!("t{n}"), &format!("b{n}"), Some("dl")).unwrap()
    }

    #[test]
    fn insert_then_list_returns_newest_first() {
        let db = db();
        add(&db, 1);
        add(&db, 2);
        let all = list(&db);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].title, "t2");
        assert_eq!(all[1].title, "t1");
        assert!(!all[0].read);
        assert_eq!(all[0].kind, "hash");
        assert_eq!(all[0].level, "info");
        assert_eq!(all[0].download_id.as_deref(), Some("dl"));
    }

    #[test]
    fn keeps_only_the_latest_100() {
        let db = db();
        for n in 1..=150 {
            add(&db, n);
        }
        let all = list(&db);
        assert_eq!(all.len() as i64, MAX_KEPT);
        assert_eq!(all.first().unwrap().title, "t150");
        assert_eq!(all.last().unwrap().title, "t51");
    }

    #[test]
    fn mark_read_and_mark_all_read() {
        let db = db();
        let a = add(&db, 1);
        add(&db, 2);
        add(&db, 3);
        mark_read(&db, a.id);
        let read: Vec<bool> = list(&db).iter().map(|n| n.read).collect();
        assert_eq!(read, vec![false, false, true]);
        mark_all_read(&db);
        assert!(list(&db).iter().all(|n| n.read));
    }

    #[test]
    fn delete_and_clear_and_unknown_ids_are_harmless() {
        let db = db();
        let a = add(&db, 1);
        add(&db, 2);
        delete(&db, a.id);
        assert_eq!(list(&db).len(), 1);
        mark_read(&db, 9999);
        delete(&db, 9999);
        assert_eq!(list(&db).len(), 1);
        clear(&db);
        assert!(list(&db).is_empty());
    }

    #[test]
    fn critical_kinds_ignore_group_switches() {
        let off = NotifyKinds { download_complete: false, download_failed: false, hash: false, verify: false, scan: false };
        assert!(off.allows(Kind::VerifyFail));
        assert!(off.allows(Kind::ScanThreat));
        assert!(!off.allows(Kind::Hash));
        assert!(!off.allows(Kind::ScanClean));
        assert!(!off.allows(Kind::DownloadFailed));
        assert!(!off.allows(Kind::DownloadComplete));
        assert!(!off.allows(Kind::VerifyOk));
        assert!(!off.allows(Kind::ScanError));
        assert!(NotifyKinds::default().allows(Kind::Hash));
    }

    #[test]
    fn kinds_have_stable_names_and_levels() {
        assert_eq!(Kind::ScanThreat.as_str(), "scan_threat");
        assert_eq!(Kind::DownloadComplete.as_str(), "download_complete");
        assert_eq!(Kind::ScanThreat.level(), "danger");
        assert_eq!(Kind::VerifyFail.level(), "danger");
        assert_eq!(Kind::DownloadFailed.level(), "danger");
        assert_eq!(Kind::ScanClean.level(), "success");
        assert_eq!(Kind::Hash.level(), "info");
    }

    #[test]
    fn compose_uses_spec_texts() {
        let (t, b) = compose(Kind::Hash, "a.zip", Some("abc"));
        assert_eq!((t.as_str(), b.as_str()), ("SHA-256 hesaplandı", "a.zip\nabc"));
        let (t, b) = compose(Kind::ScanThreat, "a.zip", None);
        assert_eq!((t.as_str(), b.as_str()), ("Tehdit bulundu", "a.zip — Dosyayı açma."));
        let (t, b) = compose(Kind::DownloadFailed, "a.zip", Some("403"));
        assert_eq!((t.as_str(), b.as_str()), ("İndirme başarısız", "a.zip: 403"));
        let (t, b) = compose(Kind::DownloadComplete, "a.zip", None);
        assert_eq!((t.as_str(), b.as_str()), ("İndirme tamamlandı", "a.zip"));
        assert_eq!(compose(Kind::VerifyOk, "a", None).0, "Özet doğrulandı");
        assert_eq!(compose(Kind::VerifyFail, "a", None).0, "Özet uyuşmuyor");
        assert_eq!(compose(Kind::ScanClean, "a", None).0, "Virüs taraması temiz");
        let (t, b) = compose(Kind::ScanError, "a.zip", Some("x"));
        assert_eq!((t.as_str(), b.as_str()), ("Virüs taraması yapılamadı", "a.zip: x"));
    }
}
