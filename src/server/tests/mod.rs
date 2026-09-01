//! The wire tier: black-box tests driving the real middleware stack over
//! in-memory duplex pipes through the internal `run_over_streams` seam.

mod conversion;
mod dispatch;
mod lifecycle;
mod robustness;
mod staleness;
mod termination;
