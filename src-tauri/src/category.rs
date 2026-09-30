//! Dosya adından kategori çıkarır; kategori, otomatik klasör ve arayüz süzgeci için kullanılır.

/// Kategori adları aynı zamanda alt klasör adıdır.
pub const ALL: [&str; 7] = ["Video", "Müzik", "Görseller", "Belgeler", "Arşivler", "Programlar", "Diğer"];

pub fn of(filename: &str) -> &'static str {
    // Sondaki ".part" gibi geçici uzantılar sayılmaz; uzantı son noktadan sonrasıdır.
    let ext = match filename.rsplit_once('.') {
        Some((_, e)) => e.to_ascii_lowercase(),
        None => return "Diğer",
    };
    match ext.as_str() {
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "mpg" | "mpeg" | "ts" | "3gp" => "Video",
        "mp3" | "flac" | "wav" | "aac" | "ogg" | "m4a" | "wma" | "opus" => "Müzik",
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "svg" | "tiff" | "ico" | "heic" => "Görseller",
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "rtf" | "odt" | "ods" | "epub" | "csv" => "Belgeler",
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "iso" | "img" | "tgz" => "Arşivler",
        "exe" | "msi" | "msix" | "apk" | "dmg" | "deb" | "rpm" | "appimage" | "bat" | "ps1" => "Programlar",
        _ => "Diğer",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_extension_case_insensitively() {
        assert_eq!(of("Film.MKV"), "Video");
        assert_eq!(of("şarkı.mp3"), "Müzik");
        assert_eq!(of("rapor.PDF"), "Belgeler");
        assert_eq!(of("yedek.tar.gz"), "Arşivler");
        assert_eq!(of("kurulum.exe"), "Programlar");
        assert_eq!(of("foto.jpeg"), "Görseller");
        assert_eq!(of("uzantisiz"), "Diğer");
        assert_eq!(of("bilinmeyen.xyz"), "Diğer");
    }

    #[test]
    fn every_category_is_listed() {
        for name in ["a.mp4", "a.mp3", "a.png", "a.pdf", "a.zip", "a.exe", "a.zzz"] {
            assert!(ALL.contains(&of(name)));
        }
    }
}
