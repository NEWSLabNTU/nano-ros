//! Zephyr on QEMU MPS2-AN385 (Cortex-M3) — the phase-337 W2 witness.
//!
//! **Bucket (phase-329 W4): matrix consumer.** These are Runtime cells of
//! `PlatformId::ZephyrQemuCortexM` in `matrix::CELLS` — the three pubsub cells
//! and the boot-report cell (issue 1036) — and nothing else.
//!
//! ## Why this platform exists at all
//!
//! Before phase-337 W2, "Zephyr" in this repo meant exactly one configuration:
//! `native_sim/native/64`. That build offloads sockets to the host
//! (`CONFIG_NET_SOCKETS_OFFLOAD=y`), so Zephyr's own IP stack never runs, no
//! ethernet driver is involved, and pointers are the host's 64 bits. RFC-0064
//! calls that a board, not a platform — and the gap is not theoretical. Bringing
//! this witness up cost five real defects, every one invisible to native_sim:
//! a 32-bit `size_t`/`uintptr_t` header conflict, an atomics feature gated on an
//! arch list (which said yes to a target that has native CAS), a Rust staticlib
//! with no allocator or panic handler off the std path, a cmake feature string
//! computed in two places, and a board with no entropy device.
//!
//! So what these tests assert is narrow and deliberate: an image built for a
//! 32-bit Cortex-M3, running Zephyr's IN-KERNEL IP stack over the `eth_smsc911x`
//! driver, opens a zenoh session through SLIRP to a router on the host and
//! publishes.
//!
//! ## Three pubsub cells, one per language
//!
//! C, C++ and Rust. This section used to say a Rust cell was impossible here:
//! the pinned `zephyr-lang-rust` could not compile the `zephyr` crate for any
//! board whose devicetree has gpio nodes — a five-argument `GpioPin::new`
//! against a six-argument signature (**issue 0432**), which an385 has and
//! native_sim does not. 0432 was resolved 2026-08-12 by phase-346 W2/W3, and
//! `matrix::CELLS` had declared the Rust cell `Runtime` since phase-346 W3 with
//! nothing running it — the kind of undeclared gap RFC-0051 exists to prevent.
//!
//! ## Prerequisites
//!
//! - `just zephyr build-fixtures` (the west leaves lane builds the
//!   `build-cortex-m-*` leaves — `scripts/build/zephyr-fixture-leaves.sh`)
//! - `qemu-system-arm` with `mps2-an385` machine support
//! - a zenoh router — ROS's `rmw_zenohd`, resolved by `ros_zenohd_path`
//!   (nano-ros ships none; RFC-0075). `just zenohd` starts one.
//!
//! Run with: `cargo nextest run -p nros-tests --test zephyr_cortex_m_qemu`

use nros_tests::{
    alloc::port_of,
    fixtures::{
        QemuProcess, RequireFixture, Rmw, ZenohRouter, build_zephyr_cortex_m_c_boot_report,
        build_zephyr_cortex_m_example, is_qemu_available, is_zenohd_available,
        qemu_monitor_command,
    },
    matrix::{Lang, PlatformId, Workload},
};
use std::{
    process::Command,
    time::{Duration, Instant},
};

/// The talker needs ~2 s for `net_config` to assign the static address, then a
/// TCP connect to the host router, then the publish loop. 90 s is the same
/// budget the FreeRTOS MPS2 lane uses on the identical machine and NIC.
const BOOT_BUDGET: Duration = Duration::from_secs(90);

/// Budget for a cell that waits on the TALKER line rather than the net line.
///
/// Deliberately far below `BOOT_BUDGET`: the board reaches "Network ready" at
/// ~2.15 s and the talker's timer period is 500 ms, so a healthy image publishes
/// within a few seconds. Waiting 90 s for a line a HALTED image will never print
/// just converts a fast, legible failure into nextest's 60 s per-test timeout,
/// which reports "TIMEOUT" and hides the fault text in the output.
const PUBLISH_BUDGET: Duration = Duration::from_secs(15);

/// Zephyr's `net_config` announcing the static address from
/// `cmake/zephyr/mps2-an385.conf`. This is the platform's distinguishing
/// evidence: it can only be printed by the in-kernel IP stack driving a real
/// ethernet controller, which is precisely what `native_sim`'s offloaded
/// sockets never do.
const NET_STACK_READY_MARKER: &str = "IPv4 address: 10.0.2.15";

/// One cell's body. `lang` selects the example tree AND the router port — the
/// port comes from the matrix allocator rather than a literal, so a future
/// platform-index change moves the bake and the test together instead of
/// leaving one of them behind.
/// `wait_marker` is per-cell because the two log orderings genuinely differ, and
/// the difference is not cosmetic — it decides when QEMU is killed.
///
/// For C/C++ the talker's first `Publishing:` reaches the stream BEFORE the
/// boot banner (Zephyr's logging subsystem flushes on its own schedule), so
/// waiting on the talker line returns at ~0.1 s with `net_config` still
/// unflushed and the driver assertion then fails a healthy run. Those cells wait
/// on the NET line, making the accumulated output a superset.
///
/// Rust flushes the other way round: `net_config` lands first and the publishes
/// follow. Waiting on the net line there kills the guest ~immediately after
/// "Network ready", BEFORE the 500 ms timer has fired — which reads exactly like
/// issue 0531's dead-clock symptom and is not it. That cell waits on the TALKER
/// line, so its accumulated output is the superset instead.
fn run_pubsub_cell(lang: &str, matrix_lang: Lang, wait_marker: &str, budget: Duration) {
    if !is_qemu_available() {
        nros_tests::unmet!("qemu-system-arm not found");
    }
    if !is_zenohd_available() {
        nros_tests::unmet!("zenohd not found — run `just setup` / source ./activate.sh");
    }

    // issue 0807 — this used to `skip!` on ANY resolver error, with a message
    // that said "not prebuilt" whatever the cause. Two things were wrong with
    // that. Since issue 0584 an absent IN-LANE fixture is a hard failure, not a
    // skip: the run is gated, so the build already promised these exist.
    // And the resolver's other verdict is STALE, which the skip relabelled as
    // missing — so a stale image reported as "not prebuilt", was filtered out
    // as a setup condition, and the cell silently stopped running. It sat that
    // way through five consecutive stale verdicts (issue 0806).
    //
    // Out-of-lane is already handled INSIDE the resolver, which raises its own
    // `[SKIPPED]`, so anything that reaches this `Err` is a genuine failure and
    // must be loud.
    let binary = build_zephyr_cortex_m_example(lang, "talker", Rmw::Zenoh).unwrap_or_else(|e| {
        panic!(
            "zephyr/{lang}/talker for mps2_an385 did not resolve. The lane gate \
             already asserted this fixture is built, so this is a real failure, \
             not a setup condition — read the verdict before rebuilding: {e:?}"
        )
    });

    // Bind 0.0.0.0, not loopback: the guest reaches the host through SLIRP's
    // 10.0.2.2 gateway, and a loopback-only listener leaves those SYNs
    // unreachable. The image has the matching locator baked in at build time
    // (`CONFIG_NROS_ZENOH_LOCATOR`), because a Cortex-M image has no env to read
    // one from — so this port and the fixture's must agree, and both derive from
    // `port_of`.
    let port = port_of(PlatformId::ZephyrQemuCortexM, matrix_lang, Workload::Pubsub);
    let _router = ZenohRouter::start_slirp(port)
        .unwrap_or_else(|e| panic!("failed to start zenohd on {port} for the SLIRP guest: {e:?}"));

    let mut qemu = QemuProcess::start_mps2_an385_networked(&binary)
        .expect("spawn Zephyr Cortex-M zenoh talker");

    // Wait on the DRIVER's line, not the talker's, even though the talker's is
    // what the cell is nominally about. The console is muxed and Zephyr's
    // logging subsystem flushes on its own schedule, so the first
    // `Publishing:` reliably lands in the stream BEFORE the boot banner — a
    // wait on the talker pattern returns at ~0.1 s with the `net_config` lines
    // still unflushed, and the driver assertion below then fails on a run that
    // is in fact perfectly healthy. Waiting for the later-flushed line makes the
    // accumulated output a superset, so both assertions see what they need.
    let output = qemu.collect_until(wait_marker, budget);
    qemu.kill();

    eprintln!("Zephyr mps2_an385 {lang} zenoh talker output:\n{output}");

    // Asserted separately from the publishes on purpose: this line is the
    // evidence that Zephyr's OWN stack and the smsc911x driver ran — the whole
    // reason this platform is distinct from native_sim. A talker that published
    // without it would mean the witness had quietly stopped being one.
    assert!(
        output.contains(NET_STACK_READY_MARKER),
        "Zephyr's in-kernel net stack never assigned the static address — the \
         eth_smsc911x driver did not come up.\nOutput:\n{output}"
    );
    nros_tests::output::assert_talker(&output, 1);
}

#[test]
fn zephyr_cortex_m_c_zenoh_pubsub_e2e() {
    run_pubsub_cell("c", Lang::C, NET_STACK_READY_MARKER, BOOT_BUDGET);
}

#[test]
fn zephyr_cortex_m_cpp_zenoh_pubsub_e2e() {
    run_pubsub_cell("cpp", Lang::Cpp, NET_STACK_READY_MARKER, BOOT_BUDGET);
}

/// The Rust cell of this board, which `matrix::CELLS` has declared `Runtime`
/// since phase-346 W3 while no test ran it — a Runtime row nothing executes is
/// the lie RFC-0051 exists to prevent.
///
/// It is also the only way to CONFIRM issue 0531 today. That bug —
/// `k_cycle_get_64()` returning a silent permanent 0 below 60 MHz, so every
/// periodic callback dies while subscriptions keep working — was fixed by
/// RFC-0073 (`platform.c` now falls back to the tick clock) but never verified
/// on hardware, because verification needs a sub-60 MHz board and this is it at
/// 12 MHz. The talker publishes from a 500 ms `nros_cpp_timer_create`, so
/// `assert_talker` IS the clock assertion: a dead clock produces no publishes.
///
/// The C and C++ cells cannot supply that evidence — issue 0552 has both
/// branching to `PC=0` right after net init on this same board, which kills the
/// image before any timer runs. Rust is unaffected there, which is what makes
/// this cell the witness.
#[test]
fn zephyr_cortex_m_rust_zenoh_pubsub_e2e() {
    run_pubsub_cell(
        "rust",
        Lang::Rust,
        nros_tests::output::TALKER_READY_MARKER,
        PUBLISH_BUDGET,
    );
}

/// How long the boot-report cell waits for the record to name the exhaustion.
/// The image fails during its FIRST registration, right after the session
/// opens (~2 s of `net_config`, then a TCP connect). Below nextest's 60 s
/// kill on purpose (`.config/nextest.toml` default profile), so a record that
/// never names the exhaustion fails with the last decode rather than a bare
/// TIMEOUT.
const BOOT_REPORT_BUDGET: Duration = Duration::from_secs(40);

/// Issue 1036 — the (ZephyrQemuCortexM, C, BootReport) cell: the boot report
/// is the only witness on a board with no console, and this reads it the way
/// an operator does on the MR-CANHUBK344.
///
/// The image is `examples/zephyr/c/talker` with
/// `fixtures/zephyr-boot-report/arena-exhausted.conf`: no console, no log, the
/// boot report on, arena exhaustion fatal, and a 32-byte executor arena that
/// the talker's first registration (48 B) cannot fit. Nothing is printed. The
/// record is dumped from guest RAM with the QEMU monitor's `pmemsave` — the
/// emulator's `savemem` — at the address and length
/// `scripts/read-boot-report.py --addr-only` resolves from the ELF, and the
/// same script decodes it. So every step a board run takes is exercised here
/// except the probe itself: the symbol resolution, the dump length, the
/// decode, the operator's verdict, and the absence of any console.
///
/// It also asserts the image HALTED in the fatal path rather than running on
/// with the entity missing — the `CONFIG_NROS_ARENA_EXHAUSTION_IS_FATAL`
/// half, which is what keeps the record from being overwritten on a board.
#[test]
fn zephyr_cortex_m_c_boot_report_names_arena_exhaustion() {
    if !is_qemu_available() {
        nros_tests::unmet!("qemu-system-arm not found");
    }
    if !is_zenohd_available() {
        nros_tests::unmet!("zenohd not found — run `just setup` / source ./activate.sh");
    }

    let binary = build_zephyr_cortex_m_c_boot_report()
        .require("zephyr/c/talker for mps2_an385, console-less with an exhausted arena");
    let root = nros_tests::project_root();
    let decoder = root.join("scripts/read-boot-report.py");

    // Where the record is, from the image's own symbol table — the same line
    // an operator pastes into `savemem`.
    let addr_only = Command::new("python3")
        .arg(&decoder)
        .arg("--addr-only")
        .arg(&binary)
        .output()
        .expect("run read-boot-report.py --addr-only");
    let addr_line = String::from_utf8_lossy(&addr_only.stdout)
        .trim()
        .to_string();
    assert!(
        addr_only.status.success(),
        "read-boot-report.py --addr-only failed on {}:\n{}",
        binary.display(),
        String::from_utf8_lossy(&addr_only.stderr)
    );
    let (addr, len) = addr_line
        .split_once(' ')
        .unwrap_or_else(|| panic!("--addr-only printed `{addr_line}`, not `<addr> <len>`"));

    // The image opens its session before it registers anything, so without a
    // router it fails at stage 2 (transport) and never reaches the arena.
    let port = port_of(PlatformId::ZephyrQemuCortexM, Lang::C, Workload::BootReport);
    let _router = ZenohRouter::start_slirp(port)
        .unwrap_or_else(|e| panic!("failed to start zenohd on {port} for the SLIRP guest: {e:?}"));

    let scratch = tempfile::tempdir().expect("scratch dir for the monitor socket and dump");
    let socket = scratch.path().join("monitor.sock");
    let dump = scratch.path().join("report.bin");
    let mut qemu = QemuProcess::start_mps2_an385_networked_with_monitor(&binary, &socket)
        .expect("spawn the console-less Zephyr Cortex-M image");

    // Poll: dump, decode, until the decoder names the exhaustion. The decoder
    // exits non-zero on ANY failure verdict, so its status is not the signal;
    // its text is.
    let deadline = Instant::now() + BOOT_REPORT_BUDGET;
    let monitor = |cmd: &str| qemu_monitor_command(&socket, cmd, Duration::from_secs(5));
    let mut verdict = String::new();
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(500));
        let save = format!("pmemsave {addr} {len} \"{}\"", dump.display());
        if monitor(&save).is_err() {
            continue; // QEMU not listening yet
        }
        let decoded = Command::new("python3")
            .arg(&decoder)
            .arg(&binary)
            .arg(&dump)
            .output()
            .expect("run read-boot-report.py");
        verdict = format!(
            "{}{}",
            String::from_utf8_lossy(&decoded.stdout),
            String::from_utf8_lossy(&decoded.stderr)
        );
        if verdict.contains(nros_tests::output::BOOT_REPORT_ARENA_EXHAUSTED) {
            break;
        }
    }

    // Halted, and in the fatal path: the program counter sits inside one of
    // the symbols that path ends in, and stays there.
    let fatal = fatal_path_ranges(&binary);
    let pc_reads: Vec<Option<u32>> = (0..2)
        .map(|i| {
            if i > 0 {
                std::thread::sleep(Duration::from_millis(500));
            }
            monitor("info registers")
                .ok()
                .and_then(|r| program_counter(&r))
        })
        .collect();
    let cmdline = qemu.command_line().to_string();
    qemu.kill();
    eprintln!("boot report of the halted guest (PC {pc_reads:x?}):\n{verdict}");

    assert!(
        verdict.contains(nros_tests::output::BOOT_REPORT_ARENA_EXHAUSTED),
        "the boot report never named the arena exhaustion within {BOOT_REPORT_BUDGET:?}.\n\
         QEMU: {cmdline}\nlast decode:\n{verdict}"
    );
    // The record names the place, the shortfall and the knob — the three
    // things an operator needs.
    //
    // Issue 1877 — checked as ARITHMETIC, not as the numbers one build
    // printed. The literals here were `short by 16 bytes` / `>= 48`, i.e. the
    // size of the first entity's arena claim on the day they were recorded,
    // and they went red the day that entity grew to 56 bytes with the decoder
    // still exactly right. What must hold whatever the claim's size: the
    // shortfall is the claim minus the arena's capacity, and the knob the
    // report tells you to set is at least the claim.
    assert!(
        verdict.contains("RegisteringEntities"),
        "the decode does not name the stage.\nDecode:\n{verdict}"
    );
    let number_after = |key: &str| -> Option<u64> {
        let rest = &verdict[verdict.find(key)? + key.len()..];
        rest.trim_start()
            .split(|c: char| !c.is_ascii_digit())
            .next()?
            .parse()
            .ok()
    };
    let claim = number_after("an allocation of")
        .unwrap_or_else(|| panic!("the decode names no allocation size.\nDecode:\n{verdict}"));
    let capacity = number_after("arena capacity")
        .unwrap_or_else(|| panic!("the decode names no arena capacity.\nDecode:\n{verdict}"));
    let short = number_after("short by")
        .unwrap_or_else(|| panic!("the decode names no shortfall.\nDecode:\n{verdict}"));
    let knob = number_after("set NROS_EXECUTOR_ARENA_SIZE >=")
        .unwrap_or_else(|| panic!("the decode names no knob value.\nDecode:\n{verdict}"));
    assert!(
        claim > capacity,
        "a claim of {claim} B fits a {capacity} B arena; that is no exhaustion.\nDecode:\n{verdict}"
    );
    assert_eq!(
        short,
        claim - capacity,
        "the shortfall is not the claim minus the capacity.\nDecode:\n{verdict}"
    );
    assert_eq!(
        knob, claim,
        "the knob the report names is not the claim that failed.\nDecode:\n{verdict}"
    );
    for (i, pc) in pc_reads.iter().enumerate() {
        let pc = pc.unwrap_or_else(|| {
            panic!("could not read the program counter (read {i}) over the QEMU monitor")
        });
        assert!(
            fatal.iter().any(|(_, lo, hi)| (*lo..*hi).contains(&pc)),
            "after the exhaustion the guest is at PC {pc:#x}, outside the fatal path \
             {fatal:#x?} — the image ran on instead of halting \
             (CONFIG_NROS_ARENA_EXHAUSTION_IS_FATAL).\nDecode:\n{verdict}"
        );
    }
}

/// Address ranges of the symbols Zephyr's fatal path ends in, from the ELF.
fn fatal_path_ranges(elf: &std::path::Path) -> Vec<(String, u32, u32)> {
    const FATAL_PATH: &[&str] = &["k_sys_fatal_error_handler", "arch_system_halt"];
    let out = ["nm", "arm-zephyr-eabi-nm", "arm-none-eabi-nm", "llvm-nm"]
        .iter()
        .find_map(|tool| {
            Command::new(tool)
                .args(["-S", "--defined-only"])
                .arg(elf)
                .output()
                .ok()
                .filter(|o| o.status.success())
        })
        .expect("no usable `nm` (tried nm, arm-zephyr-eabi-nm, arm-none-eabi-nm, llvm-nm)");
    let ranges: Vec<(String, u32, u32)> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            let [addr, size, _, name] = parts[..] else {
                return None;
            };
            if !FATAL_PATH.contains(&name) {
                return None;
            }
            // Thumb: clear the interworking bit nm may carry on a function.
            let lo = u32::from_str_radix(addr, 16).ok()? & !1;
            let hi = lo + u32::from_str_radix(size, 16).ok()?;
            Some((name.to_string(), lo, hi))
        })
        .collect();
    assert!(
        !ranges.is_empty(),
        "{} defines none of {FATAL_PATH:?}, so the halt cannot be located",
        elf.display()
    );
    ranges
}

/// `R15` out of the QEMU monitor's `info registers` for an Arm M-profile CPU.
fn program_counter(registers: &str) -> Option<u32> {
    let at = registers.find("R15=")?;
    let hex: String = registers[at + 4..]
        .chars()
        .take_while(char::is_ascii_hexdigit)
        .collect();
    u32::from_str_radix(&hex, 16).ok()
}
