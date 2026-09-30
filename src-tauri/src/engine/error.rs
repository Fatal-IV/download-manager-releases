#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("Ağ hatası: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Sunucu {0} durum kodu döndürdü")]
    Status(u16),
    #[error("Dosya hatası: {0}")]
    Io(#[from] std::io::Error),
    #[error("Sunucu çok fazla istek nedeniyle (429) bağlantıyı reddetti")]
    Throttled { retry_after: Option<u64> },
    #[error("Sunucu Range isteğini yok saydı; kaynak değişmiş olabilir")]
    ResourceChanged,
    #[error("Bağlantı beklenenden erken koptu")]
    Truncated,
}

impl EngineError {
    /// Geçici olabilecek hatalar yeniden denenir; 4xx ve dosya hataları denenmez.
    pub fn is_retryable(&self) -> bool {
        match self {
            EngineError::Http(_) | EngineError::Truncated => true,
            EngineError::Status(c) => *c >= 500 || *c == 429 || *c == 408,
            // Throttled kendi mantığıyla (bağlantı sayısını azaltarak) yönetilir.
            EngineError::Io(_) | EngineError::ResourceChanged | EngineError::Throttled { .. } => false,
        }
    }
}
