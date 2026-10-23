//! Re-export of PAL Shared Memory contracts for the TagBus subsystem.

pub use mythos_pal::{
    DefaultShm, MacOsShm, MemoryShm, PalError, PlatformShm, PosixShm, WindowsShm,
};
