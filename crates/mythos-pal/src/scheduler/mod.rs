pub mod linux;
pub mod macos;
pub mod noop;
pub mod windows;

pub use linux::LinuxScheduler;
pub use macos::MacOsScheduler;
pub use noop::NoopScheduler;
pub use windows::WindowsScheduler;

cfg_if::cfg_if! {
    if #[cfg(target_os = "linux")] {
        pub type DefaultScheduler = LinuxScheduler;
    } else if #[cfg(target_os = "windows")] {
        pub type DefaultScheduler = WindowsScheduler;
    } else if #[cfg(target_os = "macos")] {
        pub type DefaultScheduler = MacOsScheduler;
    } else {
        pub type DefaultScheduler = NoopScheduler;
    }
}
