use serde::{Deserialize, Serialize};

/// Çok küçük parçalar bağlantı kurma maliyetine değmez.
pub const MIN_SEGMENT: u64 = 256 * 1024;

/// Dosyanın `[start, end]` (ikisi de dahil) bayt aralığı ve o aralıktan inen miktar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    pub start: u64,
    pub end: u64,
    pub downloaded: u64,
}

impl Segment {
    pub fn len(&self) -> u64 {
        self.end - self.start + 1
    }

    pub fn is_done(&self) -> bool {
        self.downloaded >= self.len()
    }

    /// Devam ederken isteğin başlayacağı bayt.
    pub fn resume_from(&self) -> u64 {
        self.start + self.downloaded
    }
}

/// `total` baytı en fazla `parts` parçaya böler; hiçbir parça `MIN_SEGMENT`'ten küçük olmaz.
/// Parçalar boşluksuz ve çakışmasızdır; son parça kalanı alır.
pub fn plan(total: u64, parts: u32) -> Vec<Segment> {
    if total == 0 {
        return Vec::new();
    }
    let max_by_size = total.div_ceil(MIN_SEGMENT).max(1);
    let n = u64::from(parts.max(1)).min(max_by_size);
    let base = total / n;
    let mut out = Vec::with_capacity(n as usize);
    let mut start = 0;
    for i in 0..n {
        let len = if i == n - 1 { total - start } else { base };
        out.push(Segment { start, end: start + len - 1, downloaded: 0 });
        start += len;
    }
    out
}

/// Sürmekte olan bir parçanın kalan kısmını bölecek bayt konumu (yeni parçanın başlangıcı).
/// `pos` bir sonraki inecek bayttır, `end` parçanın son baytı (dahil). Kalan kısım iki yarısı da
/// `MIN_SEGMENT`'ten küçük kalacaksa bölünmez.
pub fn split_point(pos: u64, end: u64) -> Option<u64> {
    let remaining = (end + 1).checked_sub(pos)?;
    (remaining >= 2 * MIN_SEGMENT).then(|| pos + remaining / 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_point_halves_the_remaining_bytes() {
        let (pos, end) = (1_000_000, 1_000_000 + 4 * MIN_SEGMENT - 1);
        let mid = split_point(pos, end).unwrap();
        assert_eq!(mid, pos + 2 * MIN_SEGMENT);
    }

    #[test]
    fn split_point_refuses_when_halves_would_be_too_small() {
        assert_eq!(split_point(0, 2 * MIN_SEGMENT - 2), None);
        assert!(split_point(0, 2 * MIN_SEGMENT - 1).is_some());
    }

    #[test]
    fn split_point_is_none_for_finished_segment() {
        assert_eq!(split_point(500, 499), None);
    }

    fn assert_covers(segs: &[Segment], total: u64) {
        assert_eq!(segs.first().unwrap().start, 0);
        assert_eq!(segs.last().unwrap().end, total - 1);
        for w in segs.windows(2) {
            assert_eq!(w[0].end + 1, w[1].start, "boşluk ya da çakışma var");
        }
        assert_eq!(segs.iter().map(Segment::len).sum::<u64>(), total);
    }

    #[test]
    fn splits_evenly_and_covers_everything() {
        let segs = plan(10 * 1024 * 1024, 8);
        assert_eq!(segs.len(), 8);
        assert_covers(&segs, 10 * 1024 * 1024);
    }

    #[test]
    fn last_segment_takes_the_remainder() {
        let total = 10 * 1024 * 1024 + 7;
        let segs = plan(total, 4);
        assert_covers(&segs, total);
        assert!(segs[3].len() >= segs[0].len());
    }

    #[test]
    fn small_files_use_fewer_segments() {
        assert_eq!(plan(100, 16).len(), 1);
        assert_eq!(plan(MIN_SEGMENT * 3, 16).len(), 3);
    }

    #[test]
    fn empty_file_has_no_segments() {
        assert!(plan(0, 8).is_empty());
    }

    #[test]
    fn zero_parts_is_treated_as_one() {
        assert_eq!(plan(5 * MIN_SEGMENT, 0).len(), 1);
    }
}
