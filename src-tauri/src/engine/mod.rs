//! Segmentli indirme motoru: probe → parçalara böl → paralel indir → birleştir.

pub mod download;
pub mod error;
pub mod filename;
pub mod limiter;
pub mod probe;
pub mod segment;

#[cfg(test)]
pub(crate) mod testserver;

pub use download::{prepare, run, JobSpec, JobState, SegMeta, Outcome, RunResult};
pub use error::EngineError;
pub use limiter::Limiter;
pub use probe::{probe, ProbeInfo};
pub use segment::Segment;
