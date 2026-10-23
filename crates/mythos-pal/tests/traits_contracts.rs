//! Compile-time contract assertions for MythOS Platform Abstraction Layer (PAL).
//! Enforces Send + Sync + 'static guarantees for public structures and traits.

use mythos_pal::{
    DefaultScheduler, DefaultShm, DynamicLibrary, LinuxScheduler, MacOsScheduler, MacOsShm,
    MemoryShm, NoopScheduler, PalError, PlatformLibrary, PlatformScheduler, PlatformShm, PosixShm,
    WindowsScheduler, WindowsShm,
};

fn assert_send<T: Send>() {}
fn assert_sync<T: Sync>() {}
fn assert_static<T: 'static>() {}
fn assert_send_sync_static<T: Send + Sync + 'static>() {}
fn assert_platform_shm<T: PlatformShm>() {}
fn assert_platform_scheduler<T: PlatformScheduler>() {}
fn assert_platform_library<T: PlatformLibrary>() {}

#[test]
fn test_compile_time_send_sync_contracts() {
    // PalError
    assert_send_sync_static::<PalError>();

    // Shared Memory Structures
    assert_send_sync_static::<MemoryShm>();
    assert_send_sync_static::<PosixShm>();
    assert_send_sync_static::<MacOsShm>();
    assert_send_sync_static::<WindowsShm>();
    assert_send_sync_static::<DefaultShm>();

    // Scheduler Structures
    assert_send_sync_static::<LinuxScheduler>();
    assert_send_sync_static::<MacOsScheduler>();
    assert_send_sync_static::<WindowsScheduler>();
    assert_send_sync_static::<NoopScheduler>();
    assert_send_sync_static::<DefaultScheduler>();

    // Library Loader Structures
    assert_send_sync_static::<DynamicLibrary>();

    // Trait Object Contracts
    assert_send::<Box<dyn PlatformShm>>();
    assert_sync::<Box<dyn PlatformShm>>();
    assert_static::<Box<dyn PlatformShm>>();
}

#[test]
fn test_trait_implementation_bounds() {
    assert_platform_shm::<MemoryShm>();
    assert_platform_shm::<PosixShm>();
    assert_platform_shm::<MacOsShm>();
    assert_platform_shm::<WindowsShm>();
    assert_platform_shm::<DefaultShm>();

    assert_platform_scheduler::<LinuxScheduler>();
    assert_platform_scheduler::<MacOsScheduler>();
    assert_platform_scheduler::<WindowsScheduler>();
    assert_platform_scheduler::<NoopScheduler>();
    assert_platform_scheduler::<DefaultScheduler>();

    assert_platform_library::<DynamicLibrary>();
}
