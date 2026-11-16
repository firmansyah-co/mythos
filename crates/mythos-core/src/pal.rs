//! Re-export of PAL Scheduler and Shared Memory contracts for the core daemon.

#[cfg(unix)]
pub use mythos_pal::PosixShm;
#[cfg(windows)]
pub use mythos_pal::WindowsShm;
pub use mythos_pal::{
    DefaultScheduler, DefaultShm, DynamicLibrary, LinuxScheduler, MacOsScheduler, NoopScheduler,
    PalError, PlatformLibrary, PlatformScheduler, PlatformShm, WindowsScheduler,
};
