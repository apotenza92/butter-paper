#![cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]

#[allow(dead_code)]
#[path = "../src/perf_capture_signal.rs"]
mod perf_capture_signal;

use std::sync::Mutex;

use perf_capture_signal::{CaptureSignalError, CaptureSignalGuard};

static SIGNAL_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn sigusr1_guard_consumes_one_pending_signal_and_restores_the_prior_action() {
    let _serial = SIGNAL_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let before = current_sigusr1_action();
    let guard = CaptureSignalGuard::install().unwrap();

    assert!(!guard.consume());
    assert_eq!(unsafe { libc::raise(libc::SIGUSR1) }, 0);
    assert!(guard.consume());
    assert!(!guard.consume());

    guard.restore().unwrap();
    let after = current_sigusr1_action();
    assert_same_action(&before, &after);
}

#[test]
fn sigusr1_guard_has_one_process_owner_and_drop_restores_it() {
    let _serial = SIGNAL_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let before = current_sigusr1_action();
    {
        let _guard = CaptureSignalGuard::install().unwrap();
        assert!(matches!(
            CaptureSignalGuard::install(),
            Err(CaptureSignalError::AlreadyInstalled)
        ));
    }
    let after = current_sigusr1_action();
    assert_same_action(&before, &after);

    CaptureSignalGuard::install().unwrap().restore().unwrap();
}

fn current_sigusr1_action() -> libc::sigaction {
    let mut action = std::mem::MaybeUninit::<libc::sigaction>::uninit();
    let result = unsafe { libc::sigaction(libc::SIGUSR1, std::ptr::null(), action.as_mut_ptr()) };
    assert_eq!(result, 0);
    unsafe { action.assume_init() }
}

fn assert_same_action(expected: &libc::sigaction, actual: &libc::sigaction) {
    assert_eq!(actual.sa_sigaction, expected.sa_sigaction);
    // Linux may add the private SA_RESTORER implementation flag when an
    // action is queried after restoration. Compare every public behavioral
    // flag instead of treating that kernel detail as a contract change.
    let behavioral_flags = libc::SA_NOCLDSTOP
        | libc::SA_NOCLDWAIT
        | libc::SA_NODEFER
        | libc::SA_ONSTACK
        | libc::SA_RESETHAND
        | libc::SA_RESTART
        | libc::SA_SIGINFO;
    assert_eq!(
        actual.sa_flags & behavioral_flags,
        expected.sa_flags & behavioral_flags
    );
    let maximum_signal = libc::SIGRTMAX();
    for signal in 1..=maximum_signal {
        if signal == libc::SIGKILL || signal == libc::SIGSTOP {
            continue;
        }
        assert_eq!(
            unsafe { libc::sigismember(&expected.sa_mask, signal) },
            unsafe { libc::sigismember(&actual.sa_mask, signal) },
            "signal mask membership drifted for signal {signal}"
        );
    }
}
