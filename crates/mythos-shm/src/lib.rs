//! # MythOS Lock-Free Shared Memory Engine
//!
//! Provides lock-free triple-buffered TagBus structures and memory segment management.

pub mod pal;

pub use pal::*;
