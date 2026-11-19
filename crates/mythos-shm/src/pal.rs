//! Re-export of PAL Shared Memory contracts for the TagBus subsystem.

#[cfg(windows)]
pub use mythos_pal::WindowsShm;
pub use mythos_pal::{DefaultShm, MemoryShm, PalError, PlatformShm};
#[cfg(unix)]
pub use mythos_pal::{MacOsShm, PosixShm};
