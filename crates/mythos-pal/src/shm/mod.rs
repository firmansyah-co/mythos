pub mod mem;
pub mod posix;
pub mod macos;
pub mod windows;

pub use mem::MemoryShm;
pub use posix::PosixShm;
pub use macos::MacOsShm;
pub use windows::WindowsShm;

cfg_if::cfg_if! {
    if #[cfg(target_os = "linux")] {
        pub type DefaultShm = PosixShm;
    } else if #[cfg(target_os = "windows")] {
        pub type DefaultShm = WindowsShm;
    } else if #[cfg(target_os = "macos")] {
        pub type DefaultShm = MacOsShm;
    } else {
        pub type DefaultShm = MemoryShm;
    }
}
