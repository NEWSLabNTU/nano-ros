//! Issue 1906 — the platform condvar contract, run on ThreadX Linux.
//!
//! Boots the board with no session and runs `nros_condvar_contract::run`,
//! printing one line per check through `nros-log`. ThreadX can see a mutex's
//! lock count, so the issue-1907 "held twice" refusal is checked too.

#![no_std]
#![no_main]

use condvar_contract_threadx_linux_nros_selection::nros_board_threadx_linux::ThreadxLinux;
use nros_condvar_contract::{ALL_PASSED, FAIL_PREFIX, PASS_PREFIX};
use nros_log::{Logger, Severity, log_error, log_info, register_logger};

static LOGGER: Logger = Logger::new("condvar");

// Same shape as `logging-smoke-threadx-linux`: a `no_std` + `no_main` image
// whose C-ABI `main` glibc's crt0 calls, ending a panic through the board.
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write as _;

    struct Buf {
        bytes: [u8; 192],
        used: usize,
    }
    impl core::fmt::Write for Buf {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let room = self.bytes.len() - self.used;
            let n = s.len().min(room);
            self.bytes[self.used..self.used + n].copy_from_slice(&s.as_bytes()[..n]);
            self.used += n;
            Ok(())
        }
    }

    let mut buf = Buf {
        bytes: [0u8; 192],
        used: 0,
    };
    let _ = write!(buf, "{info}");

    unsafe extern "C" {
        fn nros_platform_panic(msg: *const u8, len: usize) -> !;
    }
    // SAFETY: `buf.bytes[..used]` is initialised and outlives the diverging call.
    unsafe { nros_platform_panic(buf.bytes.as_ptr(), buf.used) }
}

#[unsafe(no_mangle)]
extern "C" fn main(_argc: i32, _argv: *const *const core::ffi::c_char) -> i32 {
    let mut passed = false;
    let _ = ThreadxLinux::run_bare(|| {
        register_logger(&LOGGER);
        LOGGER.set_level(Severity::Info);
        let mut report = |name: &str, ok: bool, detail: &str| {
            if ok {
                log_info!(&LOGGER, "{}{}: {}", PASS_PREFIX, name, detail);
            } else {
                log_error!(&LOGGER, "{}{}: {}", FAIL_PREFIX, name, detail);
            }
        };
        passed = nros_condvar_contract::run(&mut report, true);
        if passed {
            log_info!(&LOGGER, "{}", ALL_PASSED);
        }
        nros_log::flush();
        Ok::<(), &'static str>(())
    });
    if passed { 0 } else { 1 }
}
