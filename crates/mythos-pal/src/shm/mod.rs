// Each platform's shared memory is compiled only where its system calls exist. Code outside this
// crate names `DefaultShm`, the real shared memory of whatever platform it is built for, and never
// a platform's own type: a test or a binary that names `PosixShm` does not build on Windows.
#[cfg(unix)]
pub mod macos;
pub mod mem;
#[cfg(unix)]
pub mod posix;
#[cfg(windows)]
pub mod windows;

#[cfg(unix)]
pub use macos::MacOsShm;
pub use mem::MemoryShm;
#[cfg(unix)]
pub use posix::PosixShm;
#[cfg(windows)]
pub use windows::WindowsShm;

cfg_if::cfg_if! {
    if #[cfg(target_os = "linux")] {
        pub type DefaultShm = PosixShm;
    } else if #[cfg(target_os = "windows")] {
        pub type DefaultShm = WindowsShm;
    } else if #[cfg(target_os = "macos")] {
        pub type DefaultShm = MacOsShm;
    } else if #[cfg(unix)] {
        pub type DefaultShm = PosixShm;
    } else {
        compile_error!("MythOS has no shared memory for this platform");
    }
}
