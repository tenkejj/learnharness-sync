//! LearnHarness Sync's in-process, LAN-first engine. Production LearnHarness access is intentionally unavailable.
pub mod adapter;
pub mod discovery;
pub mod engine;
pub mod identity;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub mod managed_bridge;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub mod managed_profile;
pub mod network;
pub mod storage;
pub use engine::Engine;

mod sync_test;

#[cfg(all(target_os = "linux", feature = "learnharness-qa"))]
pub mod managed_qa;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod backup_inspection;
