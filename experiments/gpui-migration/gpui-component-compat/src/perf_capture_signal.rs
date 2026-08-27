use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureSignalError {
    UnsupportedTarget,
    AlreadyInstalled,
    Os(i32),
}

impl fmt::Display for CaptureSignalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedTarget => formatter.write_str(
                "the compatibility capture signal is supported only on Linux x86_64/aarch64",
            ),
            Self::AlreadyInstalled => {
                formatter.write_str("the compatibility capture signal already has an owner")
            }
            Self::Os(code) => write!(formatter, "SIGUSR1 action failed with OS error {code}"),
        }
    }
}

impl std::error::Error for CaptureSignalError {}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
mod platform {
    use std::{
        mem::MaybeUninit,
        sync::atomic::{AtomicBool, Ordering},
    };

    use super::CaptureSignalError;

    static SIGNAL_PENDING: AtomicBool = AtomicBool::new(false);
    static HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);

    extern "C" fn capture_signal_handler(_: libc::c_int) {
        SIGNAL_PENDING.store(true, Ordering::Release);
    }

    pub struct CaptureSignalGuard {
        previous: Option<libc::sigaction>,
    }

    impl CaptureSignalGuard {
        pub fn install() -> Result<Self, CaptureSignalError> {
            HANDLER_INSTALLED
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .map_err(|_| CaptureSignalError::AlreadyInstalled)?;
            SIGNAL_PENDING.store(false, Ordering::Release);

            let mut action = unsafe { std::mem::zeroed::<libc::sigaction>() };
            action.sa_sigaction = capture_signal_handler as *const () as usize;
            action.sa_flags = libc::SA_RESTART;
            if unsafe { libc::sigemptyset(&mut action.sa_mask) } != 0 {
                HANDLER_INSTALLED.store(false, Ordering::Release);
                return Err(last_os_error());
            }

            let mut previous = MaybeUninit::<libc::sigaction>::uninit();
            if unsafe { libc::sigaction(libc::SIGUSR1, &action, previous.as_mut_ptr()) } != 0 {
                HANDLER_INSTALLED.store(false, Ordering::Release);
                return Err(last_os_error());
            }

            Ok(Self {
                previous: Some(unsafe { previous.assume_init() }),
            })
        }

        pub fn consume(&self) -> bool {
            self.previous.is_some() && SIGNAL_PENDING.swap(false, Ordering::AcqRel)
        }

        pub fn restore(mut self) -> Result<(), CaptureSignalError> {
            self.restore_inner()
        }

        fn restore_inner(&mut self) -> Result<(), CaptureSignalError> {
            let Some(previous) = self.previous.take() else {
                return Ok(());
            };
            if unsafe { libc::sigaction(libc::SIGUSR1, &previous, std::ptr::null_mut()) } != 0 {
                self.previous = Some(previous);
                return Err(last_os_error());
            }
            SIGNAL_PENDING.store(false, Ordering::Release);
            HANDLER_INSTALLED.store(false, Ordering::Release);
            Ok(())
        }
    }

    impl Drop for CaptureSignalGuard {
        fn drop(&mut self) {
            let _ = self.restore_inner();
        }
    }

    fn last_os_error() -> CaptureSignalError {
        CaptureSignalError::Os(std::io::Error::last_os_error().raw_os_error().unwrap_or(-1))
    }
}

#[cfg(not(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
mod platform {
    use super::CaptureSignalError;

    pub struct CaptureSignalGuard;

    impl CaptureSignalGuard {
        pub fn install() -> Result<Self, CaptureSignalError> {
            Err(CaptureSignalError::UnsupportedTarget)
        }

        pub const fn consume(&self) -> bool {
            false
        }

        pub const fn restore(self) -> Result<(), CaptureSignalError> {
            Ok(())
        }
    }
}

pub use platform::CaptureSignalGuard;
