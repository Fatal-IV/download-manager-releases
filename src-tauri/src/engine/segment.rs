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

#[cfg(test)]
mod tests {
    use super::*;

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
