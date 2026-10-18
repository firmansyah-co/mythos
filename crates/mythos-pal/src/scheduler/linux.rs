use crate::error::PalError;
use crate::traits::PlatformScheduler;

/// Linux real-time scheduler implementation using SCHED_FIFO and CPU affinity sets.
pub struct LinuxScheduler;

impl PlatformScheduler for LinuxScheduler {
    fn set_realtime_priority(priority_level: u32) -> Result<(), PalError> {
        #[cfg(target_os = "linux")]
        {
            let max_prio = unsafe { libc::sched_get_priority_max(libc::SCHED_FIFO) };
            let min_prio = unsafe { libc::sched_get_priority_min(libc::SCHED_FIFO) };
            let clamped = (priority_level as i32).clamp(min_prio, max_prio);

            let param = libc::sched_param {
                sched_priority: clamped,
            };

            let ret = unsafe { libc::sched_setscheduler(0, libc::SCHED_FIFO, &param) };
            if ret != 0 {
                let err = std::io::Error::last_os_error();
                if err.raw_os_error() == Some(libc::EPERM) {
                    return Err(PalError::RealtimePriorityDenied {
                        priority_level,
                        reason: "Insufficient capability (CAP_SYS_NICE or root required)".into(),
                    });
                }
                return Err(PalError::RealtimePriorityDenied {
                    priority_level,
                    reason: err.to_string(),
                });
            }
            Ok(())
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err(PalError::UnsupportedPlatform {
                operation: "LinuxScheduler::set_realtime_priority".into(),
                platform: "non-linux".into(),
            })
        }
    }

    fn pin_to_core(core_id: usize) -> Result<(), PalError> {
        #[cfg(target_os = "linux")]
        {
            unsafe {
                let mut cpuset: libc::cpu_set_t = std::mem::zeroed();
                libc::CPU_ZERO(&mut cpuset);
                libc::CPU_SET(core_id, &mut cpuset);

                let thread = libc::pthread_self();
                let ret = libc::pthread_setaffinity_np(
                    thread,
                    std::mem::size_of::<libc::cpu_set_t>(),
                    &cpuset,
                );

                if ret != 0 {
                    let err = std::io::Error::from_raw_os_error(ret);
                    return Err(PalError::CorePinningFailed {
                        core_id,
                        reason: err.to_string(),
                    });
                }
            }
            Ok(())
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err(PalError::UnsupportedPlatform {
                operation: "LinuxScheduler::pin_to_core".into(),
                platform: "non-linux".into(),
            })
        }
    }

    fn sleep_nanos(nanoseconds: u64) {
        #[cfg(target_os = "linux")]
        {
            let req = libc::timespec {
                tv_sec: (nanoseconds / 1_000_000_000) as libc::time_t,
                tv_nsec: (nanoseconds % 1_000_000_000) as libc::c_long,
            };
            let mut rem = libc::timespec {
                tv_sec: 0,
                tv_nsec: 0,
            };
            unsafe {
                libc::nanosleep(&req, &mut rem);
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            std::thread::sleep(std::time::Duration::from_nanos(nanoseconds));
        }
    }
}
