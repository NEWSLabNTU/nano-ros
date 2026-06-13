//! Phase 225.O — `nros_platform::Board*` + real-runtime `BoardEntry`
//! for the CI-runnable ESP32-C3 QEMU (OpenETH) board.
//!
//! This is the ESP32 sibling of `nros-board-freertos`'s `run_entry`
//! (212.N.7): it opens a **real** `Executor`, wraps it in an
//! `ExecutorNodeRuntime`, and hands the codegen-emitted setup closure a
//! live `RuntimeCtx` so `nros::main!(launch = …)` can `register()` the
//! launch-named Node pkgs against an RMW session — unlike the WiFi
//! board's `nros-board-bare-metal::run_entry` path, which uses a
//! `NullNodeRuntime` (every `register()` errors at runtime).
//!
//! ## Surface
//!
//! - [`Esp32QemuEntry`] — ZST implementing the four 212.N.1 board traits
//!   (`BoardInit` parameterless, `BoardPrint`, `BoardExit`) plus
//!   [`BoardEntry`]. The macro routes `deploy = "esp32-qemu"` here.
//! - `BoardEntry::run` builds a board [`Config`] (compile-time locator /
//!   domain via `option_env!`, default MAC / IP / gateway matching the
//!   single-node esp32 example), brings up hardware + transport via
//!   [`crate::node::init_hardware`], registers the zenoh RMW backend,
//!   opens the executor, registers the launch node set through the
//!   closure, then spins forever (`-> !` — ESP32 has no host exit).
//!
//! The `[esp_hal::main]` entry point itself is emitted by the
//! `nros-macros` `Framework::Esp32` branch (esp-riscv-rt's `_start`
//! requires the esp-hal entry registration; a bare `extern "C" fn main`
//! would not boot), so this crate only provides the board ZST + driver.

use nros::{ExecutorConfig, node_runtime::ExecutorNodeRuntime};
use nros_platform::{
    BoardEntry, BoardExit, BoardInit, BoardPrint, NodeDispatchRuntime, RuntimeCtx,
};

use crate::config::Config;

// Phase 213.E.2 shape — locator + domain_id are compile-time
// overridable via `NROS_LOCATOR` / `NROS_DOMAIN_ID`. `option_env!` is
// `#![no_std]`-clean and folds to a constant. Default port 7454 is the
// esp32 zenohd port (CLAUDE.md per-platform table); the slirp gateway
// `10.0.2.2` forwards to the host loopback under
// `qemu-system-riscv32 -nic user,model=open_eth`.
const LOCATOR: &str = match option_env!("NROS_LOCATOR") {
    Some(s) => s,
    None => "tcp/10.0.2.2:7454",
};
const DOMAIN_ID: u32 = match option_env!("NROS_DOMAIN_ID") {
    // `u32::from_str_radix` is not yet `const`-stable on our MSRV; accept
    // only single-digit decimals (covers the CI ROS_DOMAIN_ID 0..=9
    // range). Matches the single-node esp32 example.
    Some(s) if s.len() == 1 => (s.as_bytes()[0] - b'0') as u32,
    _ => 0,
};

/// CI-runnable ESP32-C3 QEMU board ZST (Phase 225.O Entry surface).
///
/// Distinct from [`crate::node::Esp32Qemu`], the Phase 173.1 ZST that
/// drives the legacy `run(config, f)` free fn through
/// `nros-board-common`. This one carries the 212.N.1 `nros_platform`
/// trait set + the real-runtime `BoardEntry` the workspace Entry macro
/// expects.
pub struct Esp32QemuEntry;

impl BoardInit for Esp32QemuEntry {
    fn init_hardware() {
        // The real bring-up (esp-hal init + heap + RNG + OpenETH /
        // smoltcp) needs the board `Config` (MAC / IP / gateway), so it
        // runs inside `BoardEntry::run` via `crate::node::init_hardware`.
        // The parameterless 212.N.1 hook just installs the monotonic
        // clock so a caller using the trait directly still gets `sleep`.
        nros_platform_esp32_qemu::sleep::init_clock();
    }
}

impl BoardPrint for Esp32QemuEntry {
    fn println(args: core::fmt::Arguments<'_>) {
        esp_println::println!("{}", args);
    }
}

impl BoardExit for Esp32QemuEntry {
    fn exit_success() -> ! {
        // ESP32 has no host-side process exit — spin forever. The test
        // harness kills QEMU once it observes the completion / publish
        // banner.
        #[allow(clippy::empty_loop)]
        loop {
            core::hint::spin_loop();
        }
    }

    fn exit_failure() -> ! {
        #[allow(clippy::empty_loop)]
        loop {
            core::hint::spin_loop();
        }
    }
}

impl BoardEntry for Esp32QemuEntry {
    fn run<F, E>(setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        Self::run_with_config(Self::default_config(), setup)
    }

    /// Phase 244 E5 / issue #48 — overlay the `nros::main!()` deploy block
    /// (`[package.metadata.nros.deploy.esp32-qemu]`: locator / ip / gateway /
    /// domain_id) onto the board default before boot, so the firmware dials the
    /// deploy-named endpoint instead of the inert compiled-in default. Fields the
    /// deploy block omits keep the board default. (`netmask` maps to the board's
    /// CIDR `prefix`, left at default; the smoltcp `mac_addr` stays board-internal.)
    fn run_with_deploy<F, E>(deploy: &nros_platform::DeployOverlay, setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        let mut config = Self::default_config();
        if let Some(loc) = deploy.locator {
            config.zenoh_locator = loc;
        }
        if let Some(ip) = deploy.ip {
            config.ip = ip;
        }
        if let Some(gw) = deploy.gateway {
            config.gateway = gw;
        }
        if let Some(d) = deploy.domain_id {
            config.domain_id = d;
        }
        Self::run_with_config(config, setup)
    }
}

impl Esp32QemuEntry {
    /// Board default `Config`: MAC / IP / gateway are board-internal smoltcp
    /// knobs (slirp 10.0.2.0/24, talker IP 10.0.2.50) baked verbatim from the
    /// single-node esp32 example; locator + domain are the compile-time-
    /// overridable consts above.
    fn default_config() -> Config {
        Config {
            mac_addr: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
            ip: [10, 0, 2, 50],
            prefix: 24,
            gateway: [10, 0, 2, 2],
            zenoh_locator: LOCATOR,
            domain_id: DOMAIN_ID,
        }
    }

    /// Shared boot body for [`BoardEntry::run`] + [`BoardEntry::run_with_deploy`].
    fn run_with_config<F, E>(config: Config, setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        // Bring up ESP32-C3 peripherals + heap + RNG + OpenETH/smoltcp
        // transport, then route nros log records to the console.
        crate::node::init_hardware(&config);
        crate::node::register_log_writer();

        // Bare-metal targets do not walk `.init_array`, so register the
        // RMW backend explicitly before `Executor::open` (Phase 104.A).
        nros_rmw_zenoh::register().expect("Failed to register RMW backend");

        // Open the executor + wrap it in the dispatch runtime. Locator /
        // domain come from `Config` (NOT env — embedded libc `getenv`
        // has no host trampoline on QEMU). `clock_us` feeds the timer
        // wheel the talker's 1 Hz publisher rides on.
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("nros_app")
            .clock_us(nros_platform_esp32_qemu::clock::clock_us);
        let executor = match nros::Executor::open(&exec_config) {
            Ok(executor) => executor,
            Err(err) => {
                esp_println::println!("");
                esp_println::println!("Executor::open failed: {:?}", err);
                Self::exit_failure();
            }
        };
        let mut crt = ExecutorNodeRuntime::from_executor(executor);
        let mut runtime = RuntimeCtx::with_runtime(&mut crt);

        match setup(&mut runtime) {
            Ok(()) => {
                esp_println::println!("");
                esp_println::println!("Application setup complete — entering spin loop.");
                // Embedded spin: ESP32 has no scheduler return path, so
                // loop forever driving the launch node set (the talker's
                // timer publishes /chatter). A `spin_once` error trips
                // the no-exit failure spin.
                loop {
                    if let Err(err) = NodeDispatchRuntime::spin_once(&mut crt, 10) {
                        esp_println::println!("");
                        esp_println::println!("spin_once error: {:?}", err);
                        Self::exit_failure();
                    }
                }
            }
            Err(e) => {
                esp_println::println!("");
                esp_println::println!("Application error: {:?}", e);
                Self::exit_failure();
            }
        }
    }
}
