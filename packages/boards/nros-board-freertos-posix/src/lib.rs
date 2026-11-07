//! # nros-board-freertos-posix
//!
//! phase-548 W1 — the Rust board crate for the FreeRTOS POSIX simulator: the
//! FreeRTOS kernel's `portable/ThirdParty/GCC/Posix` port running as a host
//! process, on the HOST's network (`[board.net] stack = "host"`).
//!
//! The boot path is the FreeRTOS family's (`nros_board_freertos::run_entry`
//! and friends, built with that crate's `posix-port` feature, so no lwIP is
//! brought up). This crate supplies the board half: the four
//! `nros_platform::board` traits over host stdio, and the C compile
//! (`build.rs`) of the kernel, the platform port and the glue.
//!
//! Same shape as the mps2 board (`nros-board-mps2-an385-freertos`), with the
//! host differences of `nros-board-threadx-linux`: stdout instead of
//! semihosting, `exit(3)` instead of a semihosting exit.

#![no_std]

// The global allocator adapter and the Rust side of the FreeRTOS platform
// port; the `nros_platform_*` C symbols come from this crate's build.rs.
extern crate nros_platform as _;

#[cfg(feature = "rmw-cyclonedds")]
extern crate nros_rmw_cyclonedds_sys;

pub use nros_board_freertos::{BaseConfig, Config};

/// Per-board marker for trait dispatch into the `nros_board_freertos::run_*`
/// family driver.
pub struct FreertosPosix;

unsafe extern "C" {
    fn nros_freertos_posix_stdout_write(s: *const u8);
    fn nros_freertos_posix_host_setup();
    fn nros_freertos_posix_exit(code: i32) -> !;
}

/// Unbuffered stdio and the platform log writer (`c/freertos_posix_rust.c`),
/// then the linked RMW backend. Runs before the scheduler starts; idempotent.
fn host_setup() {
    unsafe { nros_freertos_posix_host_setup() };
    // RFC-0071 D7 — the board links the backend, so the board registers it, as
    // `nros-board-linux` does. Before any `Executor::open`.
    #[cfg(feature = "rmw-cyclonedds")]
    if let Err(err) = nros_rmw_cyclonedds_sys::register() {
        <FreertosPosix as nros_platform::BoardPrint>::println(format_args!(
            "nros: cyclonedds RMW backend register failed: {:?}",
            err
        ));
    }
}

/// A fixed buffer that formats a line and hands C a NUL-terminated string.
/// Overlong output is truncated, never an error.
struct LineBuf {
    buf: [u8; 512],
    len: usize,
}

impl LineBuf {
    const fn new() -> Self {
        Self {
            buf: [0; 512],
            len: 0,
        }
    }

    fn as_c_str(&mut self) -> *const u8 {
        let end = self.len.min(self.buf.len() - 1);
        self.buf[end] = 0;
        self.buf.as_ptr()
    }
}

impl core::fmt::Write for LineBuf {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let room = (self.buf.len() - 1).saturating_sub(self.len);
        let n = room.min(s.len());
        self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

impl nros_platform::BoardInit for FreertosPosix {
    fn init_hardware() {
        // A host process: nothing to initialise before the scheduler.
    }
}

impl nros_platform::BoardPrint for FreertosPosix {
    fn println(args: core::fmt::Arguments<'_>) {
        use core::fmt::Write as _;
        let mut line = LineBuf::new();
        let _ = writeln!(line, "{}", args);
        unsafe { nros_freertos_posix_stdout_write(line.as_c_str()) };
    }
}

impl nros_platform::BoardExit for FreertosPosix {
    fn exit_success() -> ! {
        unsafe { nros_freertos_posix_exit(0) }
    }

    fn exit_failure() -> ! {
        unsafe { nros_freertos_posix_exit(1) }
    }
}

impl nros_platform::BoardEntry for FreertosPosix {
    fn run<F, E>(setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        host_setup();
        nros_board_freertos::run_entry::<FreertosPosix, F, E>(Config::default(), None, setup)
    }

    /// The `nros::main!()` deploy overlay (`[image.*]`: locator, domain_id, …)
    /// onto `Config::default()`, and the baked boot config. The address fields
    /// are inert here (the host owns the network); the locator is not.
    fn run_with_deploy<F, E>(deploy: &nros_platform::DeployOverlay, setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        host_setup();
        nros_board_freertos::run_entry::<FreertosPosix, F, E>(
            config_with_overlay(deploy),
            deploy.boot_config,
            setup,
        )
    }
}

fn config_with_overlay(deploy: &nros_platform::DeployOverlay) -> Config {
    let mut config = Config::default();
    config.base.apply_overlay(deploy);
    config
}

impl FreertosPosix {
    /// The no-session entry (logging / init-only images), as on the mps2 board.
    pub fn run_bare<F, E>(config: Config, setup: F) -> Result<(), E>
    where
        F: FnOnce(&Config) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        host_setup();
        nros_board_freertos::run_bare::<FreertosPosix, F, E>(config, setup)
    }

    /// The multi-sched-task entry `nros::main!` emits for a system with
    /// `[tasks.*]`; spawned sched tasks take heap stacks.
    pub fn run_sched_tasks<F, E>(
        deploy: &nros_platform::DeployOverlay,
        sched_tasks: &'static [nros_platform::SchedTaskSpec<'static>],
        sched_task_backing: &'static mut [nros_board_freertos::SchedTaskExecutorBackingSlot],
        setup: F,
    ) -> Result<(), E>
    where
        F: Fn(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E> + Copy,
        E: core::fmt::Debug,
    {
        host_setup();
        nros_board_freertos::run_sched_tasks_entry::<FreertosPosix, F, E>(
            config_with_overlay(deploy),
            deploy.boot_config,
            sched_tasks,
            sched_task_backing,
            &[],
            setup,
        )
    }

    /// [`run_sched_tasks`](Self::run_sched_tasks) with each spawned sched task's
    /// task memory, the call `nros::main!` emits for a FreeRTOS deploy.
    pub fn run_sched_tasks_with_task_memory<F, E>(
        deploy: &nros_platform::DeployOverlay,
        sched_tasks: &'static [nros_platform::SchedTaskSpec<'static>],
        sched_task_backing: &'static mut [nros_board_freertos::SchedTaskExecutorBackingSlot],
        task_memory: &'static [nros_board_freertos::SchedTaskMemoryRaw],
        setup: F,
    ) -> Result<(), E>
    where
        F: Fn(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E> + Copy,
        E: core::fmt::Debug,
    {
        host_setup();
        nros_board_freertos::run_sched_tasks_entry::<FreertosPosix, F, E>(
            config_with_overlay(deploy),
            deploy.boot_config,
            sched_tasks,
            sched_task_backing,
            task_memory,
            setup,
        )
    }
}
