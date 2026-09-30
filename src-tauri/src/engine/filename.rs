use percent_encoding::percent_decode_str;

/// Windows'ta dosya adında yasak karakterleri ve yol ayırıcıları temizler.
/// Sunucudan gelen ad güvenilmez olduğundan `../` gibi yol kaçışları da burada engellenir.
pub fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    if trimmed.is_empty() { "indirme".to_string() } else { trimmed.to_string() }
}

/// `Content-Disposition` başlığından dosya adını çıkarır (`filename*=` öncelikli).
pub fn from_content_disposition(value: &str) -> Option<String> {
    let mut plain = None;
    for part in value.split(';').map(str::trim) {
        if let Some(rest) = part.strip_prefix("filename*=") {
            // RFC 5987: charset'lang'yüzde-kodlu-değer
            let encoded = rest.splitn(3, '\'').nth(2)?;
            return Some(percent_decode_str(encoded).decode_utf8_lossy().into_owned());
        }
        if let Some(rest) = part.strip_prefix("filename=") {
            plain = Some(rest.trim_matches('"').to_string());
        }
    }
    plain
}

/// URL yolunun son parçasından ad üretir.
pub fn from_url(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let last = parsed.path_segments()?.rfind(|s| !s.is_empty())?;
    Some(percent_decode_str(last).decode_utf8_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_blocks_path_traversal_and_reserved_chars() {
        assert_eq!(sanitize("..\\..\\evil.exe"), "_.._evil.exe");
        assert_eq!(sanitize("a/b:c*d?.txt"), "a_b_c_d_.txt");
        assert_eq!(sanitize("   "), "indirme");
        assert_eq!(sanitize("..."), "indirme");
    }

    #[test]
    fn content_disposition_variants() {
        assert_eq!(
            from_content_disposition(r#"attachment; filename="test file.bin""#).as_deref(),
            Some("test file.bin")
        );
        assert_eq!(
            from_content_disposition("attachment; filename*=UTF-8''g%C3%BCn%C3%BCn%20raporu.pdf")
                .as_deref(),
            Some("günün raporu.pdf")
        );
        assert_eq!(from_content_disposition("inline"), None);
    }

    #[test]
    fn url_last_segment_is_decoded() {
        assert_eq!(from_url("https://x.com/a/b/rapor%20son.zip?x=1").as_deref(), Some("rapor son.zip"));
        assert_eq!(from_url("https://x.com/"), None);
    }
}
