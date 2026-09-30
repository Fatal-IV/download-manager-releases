//! Tüm indirmelerin paylaştığı hız sınırlayıcı (jeton kovası).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Saniyede izin verilen bayt sayısını sınırlar. Sınır 0 ise sınırsızdır ve hiçbir şey beklenmez.
#[derive(Debug)]
pub struct Limiter {
    /// Bayt/sn; 0 = sınırsız.
    rate: AtomicU64,
    /// (mevcut jeton, son güncelleme). Jeton eksiye düşebilir: borç kadar beklenir.
    bucket: Mutex<(f64, Instant)>,
}

impl Limiter {
    pub fn new(bytes_per_sec: u64) -> Self {
        Self { rate: AtomicU64::new(bytes_per_sec), bucket: Mutex::new((0.0, Instant::now())) }
    }

    pub fn set_rate(&self, bytes_per_sec: u64) {
        self.rate.store(bytes_per_sec, Ordering::Relaxed);
    }

    /// `n` baytlık veri için izin alır; sınır aşılıyorsa gereken süre kadar bekler.
    pub async fn acquire(&self, n: usize) {
        let wait = self.reserve(n);
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }

    fn reserve(&self, n: usize) -> Duration {
        let rate = self.rate.load(Ordering::Relaxed);
        if rate == 0 {
            return Duration::ZERO;
        }
        let rate = rate as f64;
        let mut b = self.bucket.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        // Uzun boşluktan sonra ani bir yığılma olmasın: en çok çeyrek saniyelik birikim.
        b.0 = (b.0 + now.duration_since(b.1).as_secs_f64() * rate).min(rate / 4.0);
        b.1 = now;
        b.0 -= n as f64;
        if b.0 >= 0.0 { Duration::ZERO } else { Duration::from_secs_f64(-b.0 / rate) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unlimited_never_waits() {
        let l = Limiter::new(0);
        let t = Instant::now();
        for _ in 0..1000 {
            l.acquire(1 << 20).await;
        }
        assert!(t.elapsed() < Duration::from_millis(100));
    }

    #[tokio::test]
    async fn limits_throughput_to_configured_rate() {
        // 1 MB/sn'de 1 MB gönderimi yaklaşık bir saniye sürmeli.
        let l = Limiter::new(1 << 20);
        let t = Instant::now();
        for _ in 0..64 {
            l.acquire(16 * 1024).await;
        }
        let e = t.elapsed();
        assert!(e >= Duration::from_millis(600), "çok hızlı: {e:?}");
        assert!(e <= Duration::from_millis(2000), "çok yavaş: {e:?}");
    }
}
