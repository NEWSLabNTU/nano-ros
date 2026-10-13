//! ThreadX Linux nros-log smoke fixture.
//!
//! Boots via the board crate so `run()` registers the ThreadX log
//! writer, then emits every severity through `nros-log`.

#![no_std]
#![no_main]

use logging_smoke_threadx_linux_nros_selection::nros_board_threadx_linux::ThreadxLinux;
use nros_log::{
    Logger, Severity, log_debug, log_error, log_fatal, log_info, log_warn, nros_trace,
    register_logger,
};

static LOGGER: Logger = Logger::new("smoke");

// Issue 1305 — a `#![no_std]` + `#![no_main]` image, the shape the board's
// descriptor states (`entry_kind = "board-run"`, issue 1759): glibc's crt0 calls
// the C-ABI `main` below, the ThreadX byte pool serves `alloc` (the selection
// package turns on the board's `image-runtime`), and this panic handler ends a
// panic through the board's `nros_platform_panic`. It was an ordinary `std`
// program, and libstd allocated before `tx_kernel_enter` created the pool —
// which the byte-pool `#[global_allocator]` answers with NULL (measured:
// `memory allocation of 4 bytes failed`, before any output). Written out
// rather than `nros::panic_to_platform!()` only because this bin does not dep
// the `nros` facade.
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
    // Phase 313 W0 (#0243) — boot via the new-family NO-SESSION `run_bare`
    // (was the legacy `run(Config, closure)`). A logging fixture opens no ROS
    // session, so `run_bare` boots the kernel + log writer and runs this closure
    // directly — `BoardEntry::run` would abort on `Executor::open` with no router.
    let _ = ThreadxLinux::run_bare(|| {
        register_logger(&LOGGER);
        // issue 0708 — deliberately NO `init(sinks::default())` here.
        //
        // This fixture used to publish its own sink list, which made it prove
        // that the platform's `nros_platform_log_write` works (issue 0420's
        // question) and nothing about whether a BOARD publishes one. It did
        // not, and library records — `log_error!` raised inside a crate whose
        // author cannot know what the board did — were dropped on this whole
        // family. Relying on the board is what makes this an assertion about
        // the board.
        //
        // If this fixture ever emits nothing again, do not add an `init` here:
        // that hides the defect it exists to catch. The boot funnel is where it
        // belongs, and `check-board-log-sink` says which one is missing it.
        LOGGER.set_level(Severity::Trace);

        nros_trace!(&LOGGER, "trace payload");
        log_debug!(&LOGGER, "debug payload");
        log_info!(&LOGGER, "info payload");
        // phase-479 W6 (RFC-0102) — one CHILD of this logger. Its record must reach
        // the platform writer under the dotted name `smoke.child`; emitted before the
        // FATAL line some harnesses stop reading at.
        match LOGGER.create_child("child") {
            Ok(child) => log_info!(child, "child payload"),
            Err(e) => log_error!(&LOGGER, "create_child failed: {}", e),
        }
        log_warn!(&LOGGER, "warn payload");
        log_error!(&LOGGER, "error payload");
        log_fatal!(&LOGGER, "fatal payload");
        nros_log::flush();

        Ok::<(), &'static str>(())
    });
    0
}
