use reqwest::header::{CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, ETAG, LAST_MODIFIED, RANGE};
use reqwest::{Client, StatusCode};

use super::error::EngineError;
use super::filename;

#[derive(Debug, Clone)]
pub struct ProbeInfo {
    /// Boyut bilinmiyorsa `None` (ör. chunked yanıt).
    pub total: Option<u64>,
    pub accepts_ranges: bool,
    pub filename: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

/// `Range: bytes=0-0` ile sunucuyu yoklar. HEAD yerine GET kullanılır çünkü bazı sunucular
/// HEAD'e yanlış yanıt verir; 206 dönmesi Range desteğinin en güvenilir kanıtıdır.
pub async fn probe(
    client: &Client,
    url: &str,
    headers: &[(String, String)],
) -> Result<ProbeInfo, EngineError> {
    // Sunucu 429 verirse birkaç kez bekleyip yeniden dene.
    let mut tries = 0;
    let resp = loop {
        let mut req = client.get(url).header(RANGE, "bytes=0-0");
        for (k, v) in headers {
            req = req.header(k, v);
        }
        let resp = req.send().await?;
        if resp.status() != StatusCode::TOO_MANY_REQUESTS || tries >= 4 {
            break resp;
        }
        tries += 1;
        let wait = super::download::retry_after_of(&resp).unwrap_or(2u64.pow(tries)).clamp(1, 30);
        tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
    };
    let status = resp.status();
    if !status.is_success() {
        return Err(EngineError::Status(status.as_u16()));
    }

    let h = resp.headers();
    let text = |name| h.get(name).and_then(|v| v.to_str().ok()).map(str::to_owned);

    let (total, accepts_ranges) = if status == StatusCode::PARTIAL_CONTENT {
        // "bytes 0-0/12345"
        let total = text(CONTENT_RANGE)
            .and_then(|v| v.rsplit('/').next().and_then(|t| t.parse::<u64>().ok()));
        (total, total.is_some())
    } else {
        // 200 dönen sunucu Range'i yok saymıştır; Accept-Ranges başlığına güvenmiyoruz.
        (text(CONTENT_LENGTH).and_then(|v| v.parse::<u64>().ok()), false)
    };

    let name = text(CONTENT_DISPOSITION)
        .and_then(|v| filename::from_content_disposition(&v))
        .or_else(|| filename::from_url(url))
        .map(|n| filename::sanitize(&n))
        .unwrap_or_else(|| "indirme".to_string());

    Ok(ProbeInfo { total, accepts_ranges, filename: name, etag: text(ETAG), last_modified: text(LAST_MODIFIED) })
}
