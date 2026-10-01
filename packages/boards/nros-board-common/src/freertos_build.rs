//! Shared `build.rs` helpers for the FreeRTOS + lwIP board family.
//!
//! phase-337 W5.d — `configure_arm_cm3` / `add_freertos_includes` /
//! `add_lwip_includes` existed twice (`nros-board-freertos/build.rs` and
//! `nros-board-mps2-an385-freertos/build.rs`), and the copies had already
//! diverged: the family crate resolved cflags from the `[arch.*]` profiles
//! (phase-338 W4) while the overlay still hardcoded a Cortex-M3 fallback — so a
//! Cortex-M7 overlay would have compiled its own board glue with M3 flags while
//! the kernel beside it got M7 flags. One spelling lives here; both build
//! scripts call it (CLAUDE.md: add ONE shared helper, never a second spelling).
//!
//! [`emit_app_config_tu`](crate::freertos_build::emit_app_config_tu) is the third de-duplication: the `NROS_APP_CONFIG` C
//! symbol was a 57-line hand-maintained C string in the overlay's `build.rs`
//! mirroring `nros_board_freertos::Config::default()` by eye. It now takes a
//! [`BaseConfig`] and a [`FreertosScheduling`] and writes the same TU from
//! them.
//!
//! phase-471 W2 — [`run_overlay`](crate::freertos_build::run_overlay) is the RUNNER the helpers above never added
//! up to. The study that opened phase-471 measured what "helpers only" cost:
//! `nros-board-mps3-an536-freertos/build.rs` and
//! `nros-board-s32z270-freertos/build.rs`, past their doc comments, were **131
//! shared lines with 5 differing ones** — the crate name, a linker-script name,
//! a board C file name — and `gcc_print_file` was copied verbatim into three
//! board scripts, hardcoded `-mcpu` flags and all. NuttX and ThreadX-RISCV had
//! had runners since phase-337 (`nuttx_ffi_build::run_nuttx`,
//! `threadx_qemu_riscv64_build::run`), and their board scripts are 3 to 17
//! lines. This module is the FreeRTOS family catching up; the overlays that
//! call it keep only what differs.

use std::{
    env,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use crate::{
    arch_flags,
    base_config::BaseConfig,
    freertos_config::{FreertosScheduling, app_stack_bytes},
};

/// The platform whose `[arch.*]` profiles supply this family's cflags —
/// `packages/platform/nros-platform-freertos/nros-platform.toml`.
// phase-349 W1 — the platform is `freertos`; the stack is a fact declared
// elsewhere. `freertos-lwip` survives as an alias in the descriptor's `names`.
const PLATFORM: &str = "freertos";

/// Shared cflag setup for every FreeRTOS + lwIP translation unit.
///
/// `-ffunction-sections` / `-fdata-sections` / `-O2` / warnings-off stay
/// built-in defaults — every FreeRTOS+lwIP consumer wants them.
///
/// Resolution order for the target flags, so the RFC-0049 ladder still runs
/// board-over-platform:
///   1. `FREERTOS_CFLAGS` — the board's explicit override (rung 1).
///   2. the matching `[arch.*]` profile for this `TARGET`.
///   3. loud failure naming the profiles that exist and what they admit.
///
/// A non-`thumb*` target (host `cargo check`, the source-metadata probe) skips
/// all of it — there is no embedded compile of substance there.
///
/// # Panics
/// When a `thumb*` target matches no `[arch.*]` profile and no
/// `FREERTOS_CFLAGS` was given. Silently compiling for the wrong CPU or FPU ABI
/// is worse than failing here (phase-195 audit (b)).
pub fn configure_cflags(build: &mut cc::Build) {
    build
        .opt_level(2)
        .flag("-ffunction-sections")
        .flag("-fdata-sections")
        .warnings(false);
    // issue 0383 — implicit-function-declaration / int-conversion as ERRORS.
    // Safe next to `warnings(false)`: that only makes cc-rs OMIT `-Wall`/
    // `-Wextra`, it passes no `-w`, and gcc enables both diagnostics by
    // default — so the gate is live on the pinned arm-none-eabi-gcc 13.2.
    nros_cc_flags::strict_decls(build);

    for flag in resolve_cflags().split_whitespace() {
        build.flag(flag);
    }
}

fn resolve_cflags() -> String {
    if let Ok(v) = env::var("FREERTOS_CFLAGS") {
        return v;
    }
    let target = env::var("TARGET").unwrap_or_default();
    // phase-372 W1 — `arm*` cross targets (Cortex-R52: `armv8r-none-eabihf`)
    // resolve through the [arch.*] profiles exactly like `thumb*` ones. The
    // old guard returned the M3 legacy default for ANYTHING non-thumb, which
    // would silently compile R-profile boards with `-mcpu=cortex-m3 -mthumb`
    // — the wrong-CPU outcome the panic below exists to prevent. Host builds
    // (x86_64…) still take the legacy default; they never reach a real
    // embedded compile (skip_cross_build guards the board build scripts).
    if !target.starts_with("thumb") && !target.starts_with("arm") {
        return "-mcpu=cortex-m3 -mthumb".to_string();
    }
    let roots = arch_flags::platform_search_path().unwrap_or_else(|| {
        panic!(
            "nros-board-freertos: TARGET=`{target}` needs arch cflags but no nano-ros \
             platform descriptor root (packages/platform, config) was found walking up \
             from CARGO_MANIFEST_DIR. Out-of-tree consumer? Set FREERTOS_CFLAGS \
             explicitly."
        )
    });
    match arch_flags::cflags_for_target(&roots, PLATFORM, &target) {
        Ok(Some(flags)) => flags.join(" "),
        Ok(None) => panic!(
            "nros-board-freertos: no [arch.*] profile of platform `{PLATFORM}` admits \
             TARGET=`{target}`.\n  declared: {}\n  Either add an [arch.*] block to \
             that platform's nros-platform.toml, or set FREERTOS_CFLAGS in the board's \
             .cargo/config.toml [env] — e.g. `-mcpu=cortex-m4 -mthumb -mfpu=fpv4-sp-d16 \
             -mfloat-abi=hard` for a Cortex-M4F.",
            arch_flags::describe_profiles(&roots, PLATFORM)
        ),
        Err(e) => panic!("nros-board-freertos: reading arch profiles: {e}"),
    }
}

/// FreeRTOS kernel + port + `FreeRTOSConfig.h` include dirs.
pub fn add_freertos_includes(
    build: &mut cc::Build,
    freertos_dir: &Path,
    port_dir: &Path,
    config_dir: &Path,
) {
    build
        .include(config_dir)
        .include(freertos_dir.join("include"))
        .include(port_dir);
}

/// lwIP core + FreeRTOS contrib-port include dirs.
pub fn add_lwip_includes(build: &mut cc::Build, lwip_dir: &Path) {
    build
        .include(lwip_dir.join("src/include"))
        .include(lwip_dir.join("contrib/ports/freertos/include"));
}

/// The app-task stack size for this build, honouring
/// `NROS_FREERTOS_APP_STACK_KB`. Emits the `rerun-if-env-changed` line.
pub fn app_stack_bytes_from_build_env() -> u32 {
    println!("cargo:rerun-if-env-changed=NROS_FREERTOS_APP_STACK_KB");
    let builtin = app_stack_bytes(None);
    // phase-400 W6 — the platform and board rungs, beneath the env front-end
    // that still wins. `None` when no lane named a platform, which is a bare
    // `cargo build`: then this is exactly the env-or-default it always was.
    match crate::platform_config::BuildRungs::from_build_env() {
        Some(rungs) => rungs.memory_value("app_stack_bytes", builtin as usize) as u32,
        None => app_stack_bytes(env::var("NROS_FREERTOS_APP_STACK_KB").ok().as_deref()),
    }
}

/// Write the board's `NROS_APP_CONFIG` definition into `out_dir` and return the
/// path, for `cc::Build::file`.
///
/// The symbol is what the C/C++ application entry reads for network bring-up
/// and task sizing (`<nros/app_config.h>` declares the type). Before phase-337
/// W5.d this was a 57-line C string literal in the MPS2 overlay's `build.rs`,
/// maintained by eye against `nros_board_freertos::Config::default()` — and it
/// had drifted by 128 KiB on `app_stack_bytes`.
pub fn emit_app_config_tu(
    out_dir: &Path,
    base: &BaseConfig,
    sched: &FreertosScheduling,
) -> PathBuf {
    let out_path = out_dir.join("nros_app_config_def.c");
    let ip = base.ip;
    let mac = base.mac;
    let gw = base.gateway;
    let nm = base.netmask;
    let body = format!(
        r#"/* GENERATED by nros_board_common::freertos_build::emit_app_config_tu —
 * do not edit. The values come from the board crate's `BaseConfig` +
 * `FreertosScheduling`, so this file cannot drift from the Rust defaults the
 * way the hand-written mirror it replaces did (phase-337 W5.d).
 *
 * `<nros/app_config.h>` is the canonical-path wrapper for the shipped
 * `nros_app_config_t` type, so there is no inlined-typedef sync obligation.
 */

#include <stdint.h>
#include <nros/app_config.h>

const nros_app_config_t NROS_APP_CONFIG = {{
    .zenoh = {{
        .locator   = "{locator}",
        .domain_id = {domain_id},
    }},
    .network = {{
        .ip      = {{ {ip0}, {ip1}, {ip2}, {ip3} }},
        .mac     = {{ 0x{mac0:02x}, 0x{mac1:02x}, 0x{mac2:02x}, 0x{mac3:02x}, 0x{mac4:02x}, 0x{mac5:02x} }},
        .gateway = {{ {gw0}, {gw1}, {gw2}, {gw3} }},
        .netmask = {{ {nm0}, {nm1}, {nm2}, {nm3} }},
        .prefix  = {prefix},
    }},
    .scheduling = {{
        .app_priority            = {app_priority},
        .zenoh_read_priority     = {zenoh_read_priority},
        .zenoh_lease_priority    = {zenoh_lease_priority},
        .poll_priority           = {poll_priority},
        .app_stack_bytes         = {app_stack_bytes}u,
        .zenoh_read_stack_bytes  = {zenoh_read_stack_bytes}u,
        .zenoh_lease_stack_bytes = {zenoh_lease_stack_bytes}u,
        .poll_interval_ms        = {poll_interval_ms}u,
    }},
}};
"#,
        locator = base.zenoh_locator,
        domain_id = base.domain_id,
        ip0 = ip[0],
        ip1 = ip[1],
        ip2 = ip[2],
        ip3 = ip[3],
        mac0 = mac[0],
        mac1 = mac[1],
        mac2 = mac[2],
        mac3 = mac[3],
        mac4 = mac[4],
        mac5 = mac[5],
        gw0 = gw[0],
        gw1 = gw[1],
        gw2 = gw[2],
        gw3 = gw[3],
        nm0 = nm[0],
        nm1 = nm[1],
        nm2 = nm[2],
        nm3 = nm[3],
        prefix = base.prefix(),
        // Issue 0623 — the struct already holds RAW FreeRTOS priorities, so
        // this emits them verbatim.
        //
        // It converted here for one commit, while the struct was still
        // normalized; the conversion has since moved to the only place that can
        // know which scale a number is on — the parser that reads it from a
        // `[node.rt]` (normalized, legacy) or `[node.rt.freertos]` (raw)
        // section. Converting at the emitter meant every OTHER consumer had to
        // convert too, which is how the C entry's saturating `clamp_prio` came
        // to disagree with the Rust entry's proportional map (16 -> 7 vs
        // 16 -> 4, flattening app/transport/poll into one priority).
        app_priority = sched.app_priority,
        zenoh_read_priority = sched.zenoh_read_priority,
        zenoh_lease_priority = sched.zenoh_lease_priority,
        poll_priority = sched.poll_priority,
        app_stack_bytes = sched.app_stack_bytes,
        zenoh_read_stack_bytes = sched.zenoh_read_stack_bytes,
        zenoh_lease_stack_bytes = sched.zenoh_lease_stack_bytes,
        poll_interval_ms = sched.poll_interval_ms,
    );
    File::create(&out_path)
        .expect("failed to create nros_app_config_def.c")
        .write_all(body.as_bytes())
        .expect("failed to write nros_app_config_def.c");
    out_path
}

// ---------------------------------------------------------------------------
// The overlay runner (phase-471 W2)
// ---------------------------------------------------------------------------

/// The shared section layout every FreeRTOS board's own linker script
/// `INCLUDE`s. It lives in the family crate, and `INCLUDE` resolves against the
/// linker's search path — so both scripts land in `OUT_DIR` and the
/// `rustc-link-search` [`run_overlay`] prints is what puts that on the path.
const SHARED_LINKER_SCRIPT: &str = "nros-freertos-cortex-m.ld";

/// Everything [`run_overlay`] resolved, handed to an overlay's hooks so a board
/// with extra archives compiles them against the SAME paths rather than
/// resolving a second time.
///
/// Every path-valued field here is read through `nros_build_paths`, which is
/// what applies issue 1280's three-valued rule (outside any checkout → keep, a
/// DIFFERENT checkout → re-root here, this one → keep). Folding the resolution
/// into the runner is what keeps that rule at one site for the whole family
/// instead of three copies of `env::var` — issue 1527 was two of those three
/// copies drifting apart.
///
/// **This MOVED three reads (`FREERTOS_DIR`, `LWIP_DIR`, `FREERTOS_CONFIG_DIR`)
/// out of the board `build.rs` files and into this one, which is a build-script
/// LIBRARY.** phase-471 W3's `check-build-script-path-resolution` reads
/// `build.rs` files, so the reads left its reach — measured, not predicted: it
/// counts 6 build scripts naming a path-valued SDK variable before W2 and 3
/// after. Nothing was laundered: all three still go through `nros_build_paths`,
/// the gate is green, and the surface it has to cover went from three copies to
/// one. Extending it to this crate is W6's open question (its three raw
/// `NUTTX_DIR` reads live next door), and this is the second site waiting on
/// that answer. `FREERTOS_PORT` is a port NAME, not a path, and stays a plain
/// `env::var`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct OverlayEnv {
    /// `OUT_DIR` — where the linker scripts and the emitted `NROS_APP_CONFIG`
    /// TU land.
    pub out_dir: PathBuf,
    /// The overlay crate's own directory.
    pub manifest_dir: PathBuf,
    /// `<manifest_dir>/config` — this board's `FreeRTOSConfig.h` / `lwipopts.h`
    /// / linker script.
    pub config_dir: PathBuf,
    /// `nros-board-freertos/config` — the family's shared headers and section
    /// layout.
    pub shared_config_dir: PathBuf,
    /// The FreeRTOS kernel root.
    pub freertos_dir: PathBuf,
    /// `<freertos_dir>/portable/<port>`, with `<port>` from `FREERTOS_PORT` or
    /// the overlay's [`Overlay::default_port`].
    pub port_dir: PathBuf,
    /// The lwIP root.
    pub lwip_dir: PathBuf,
    /// The directory holding `FreeRTOSConfig.h` — `FREERTOS_CONFIG_DIR` when
    /// set, otherwise this board's own `config/`.
    ///
    /// It cannot be `nros_build_paths::freertos_config_dir()`: that one
    /// defaults to the MPS2 board's directory for every caller, which is the
    /// wrong default for every other board.
    pub freertos_config_dir: PathBuf,
    /// `packages/api/nros-c/include`, already asserted to hold
    /// `nros/app_config.h` (issue 0365).
    pub nros_c_include: PathBuf,
}

/// An overlay's hook for archives it compiles ITSELF, before the board glue
/// and against the runner's resolved paths.
pub type ExtraArchives<'a> = &'a dyn Fn(&OverlayEnv);

/// An overlay's hook for includes and defines only its own board glue needs.
pub type ConfigureGlue<'a> = &'a dyn Fn(&OverlayEnv, &mut cc::Build);

/// What differs between the FreeRTOS board overlays, and nothing else.
///
/// A field here earned its place by being one of the 5 lines that differed
/// between the two overlays phase-471 W0 measured, or by being an extra the
/// MPS2 overlay carries that the other two do not.
///
/// Deliberately NOT `#[non_exhaustive]`: an overlay with extras builds this
/// with `..Overlay::new(..)`, which is a struct expression and therefore
/// forbidden outside the defining crate for a non-exhaustive type. Every caller
/// is an in-tree board crate, so a new field is a compile error in three files
/// rather than a silent default nobody chose.
pub struct Overlay<'a> {
    /// The crate name, for `skip_cross_build`'s message (issue 0288).
    pub crate_name: &'a str,
    /// This board's linker script, by name, in `config/`. The shared section
    /// layout is added by the runner.
    pub board_linker_script: &'a str,
    /// The `portable/<...>` port used when `FREERTOS_PORT` is unset.
    pub default_port: &'a str,
    /// The board's own C translation units, relative to the crate root.
    pub board_c_files: &'a [&'a str],
    /// Archives this overlay compiles ITSELF that the runner must name on the
    /// link line — an archive `cc::Build::compile` already emitted a link-lib
    /// line for still gets re-named here by the overlays that did so before,
    /// because a second position on ld's single pass is load-bearing for
    /// whole-archive-free member selection.
    pub extra_link_libs: &'a [&'a str],
    /// Extra archives, compiled before the board glue and against the same
    /// resolved paths. This is where the MPS2 overlay's LAN9118 netif driver
    /// and its opt-in Tonbandgeraet trace library live.
    pub extra_archives: Option<ExtraArchives<'a>>,
    /// Extra includes / defines on the board glue TU set, before the runner
    /// adds `nros-c`'s include and the board C files.
    pub configure_glue: Option<ConfigureGlue<'a>>,
}

impl<'a> Overlay<'a> {
    /// The common case: a board with a linker script, one C file, a port, and
    /// no extras. `s32z270` and `mps3-an536` are exactly this.
    pub fn new(
        crate_name: &'a str,
        board_linker_script: &'a str,
        default_port: &'a str,
        board_c_files: &'a [&'a str],
    ) -> Self {
        Self {
            crate_name,
            board_linker_script,
            default_port,
            board_c_files,
            extra_link_libs: &[],
            extra_archives: None,
            configure_glue: None,
        }
    }
}

/// Run a FreeRTOS + lwIP board overlay's whole build script.
///
/// Returns immediately when this is not a cross build (issue 0288 — the
/// source-metadata probe runs these scripts host-side, where handing the host
/// `cc` an `-mcpu=cortex-m3` kills it before rustc runs).
///
/// What it does, in the order the overlays it replaces did it: copy the board's
/// linker script and the family's shared one into `OUT_DIR` and put `OUT_DIR`
/// on the link search path; run [`Overlay::extra_archives`]; compile the board
/// glue (cflags from the `[arch.*]` profile, FreeRTOS + lwIP + `nros-c`
/// includes, the board C files, the emitted `NROS_APP_CONFIG` TU); discover
/// newlib and libgcc for the RIGHT multilib; print the rerun triggers.
///
/// # Panics
/// When a linker script cannot be copied, when `nros-c`'s header is not where
/// the workspace layout says it is (issue 0365), or when `arm-none-eabi-gcc`
/// cannot be run or cannot resolve a multilib file.
pub fn run_overlay(overlay: &Overlay<'_>) {
    // issue 0288 — skip the ARM cross-compile when host tooling builds this
    // crate (the source-metadata probe).
    if crate::host_probe::skip_cross_build(overlay.crate_name, &["thumb", "arm"]) {
        return;
    }

    let env = resolve_overlay_env(overlay.default_port);

    // --- Linker scripts ---
    // The board script `INCLUDE`s the shared section layout; both land in
    // OUT_DIR on the linker search path. The image's cargo config names
    // `-T<board_linker_script>` in rustflags.
    for (src, name) in [
        (
            env.config_dir.join(overlay.board_linker_script),
            overlay.board_linker_script,
        ),
        (
            env.shared_config_dir.join(SHARED_LINKER_SCRIPT),
            SHARED_LINKER_SCRIPT,
        ),
    ] {
        fs::copy(&src, env.out_dir.join(name))
            .unwrap_or_else(|e| panic!("copying {} into OUT_DIR: {e}", src.display()));
        println!("cargo:rerun-if-changed={}", src.display());
    }
    println!("cargo:rustc-link-search={}", env.out_dir.display());

    if let Some(extra) = overlay.extra_archives {
        extra(&env);
    }

    // --- Board C: startup + weak netif/tick hooks ---
    let mut glue = cc::Build::new();
    configure_cflags(&mut glue);
    add_freertos_includes(
        &mut glue,
        &env.freertos_dir,
        &env.port_dir,
        &env.freertos_config_dir,
    );
    add_lwip_includes(&mut glue, &env.lwip_dir);
    if let Some(configure) = overlay.configure_glue {
        configure(&env, &mut glue);
    }
    glue.include(&env.nros_c_include);
    for rel in overlay.board_c_files {
        glue.file(env.manifest_dir.join(rel));
    }

    let sched = FreertosScheduling {
        app_stack_bytes: app_stack_bytes_from_build_env(),
        ..FreertosScheduling::default()
    };
    glue.file(emit_app_config_tu(
        &env.out_dir,
        &BaseConfig::default(),
        &sched,
    ));

    // issue 0478 — cc-rs would hand arm-none-eabi-gcc the clang-only
    // `-mno-omit-leaf-frame-pointer`, which gcc REJECTS.
    nros_cc_flags::gcc_safe_frame_pointer(&mut glue);
    nros_cc_flags::header_deps::track_header_deps(&mut glue);
    glue.compile("startup");
    // issue 1580 — every file the board-glue compile OPENED becomes an edge:
    // the board C files, the board + family FreeRTOSConfig.h / lwipopts.h /
    // arch/cc.h (the board copies `#include` the family ones by relative path,
    // which the old hand list never named), and every kernel / lwIP / nros-c
    // header. An overlay's `extra_archives` compile into this same OUT_DIR and
    // declare their own (the helper consumes each depfile it reads).
    nros_cc_flags::header_deps::emit_header_deps(&env.out_dir);

    println!("cargo:rustc-link-lib=static=startup");
    for lib in overlay.extra_link_libs {
        println!("cargo:rustc-link-lib=static={lib}");
    }

    // --- Newlib (libc + nosys stubs) — multilib-correct discovery ---
    // zenoh-pico and lwIP call standard C functions; `--print-file-name` finds
    // the multilib-correct paths (`--print-sysroot` is empty on some distros).
    for file in ["libc.a", "libgcc.a"] {
        let path = gcc_print_file(file);
        let dir = Path::new(&path)
            .parent()
            .unwrap_or_else(|| panic!("arm-none-eabi-gcc returned a parentless path for {file}"));
        println!("cargo:rustc-link-search={}", dir.display());
    }
    println!("cargo:rustc-link-lib=static=c");
    println!("cargo:rustc-link-lib=static=nosys");
    println!("cargo:rustc-link-lib=static=gcc");

    // --- Rerun triggers ---
    // The compiled inputs (board C files, config headers) are declared by
    // `emit_header_deps` above (issue 1580). What stays is what no compiler
    // reads: the two linker scripts (declared where they are copied), this
    // script, and the env VALUES below.
    println!("cargo:rerun-if-changed=build.rs");
    // issue 0491 — the PATH variables resolved above (`FREERTOS_DIR`,
    // `LWIP_DIR`, `FREERTOS_CONFIG_DIR`, and whatever an overlay's extras
    // read) are NOT fingerprinted as strings: cargo compares an env value as
    // TEXT and one directory has a different spelling per leaf, per `just`,
    // and unset. Their CONTENT is what the depfiles above declare.
    println!("cargo:rerun-if-env-changed=FREERTOS_PORT");
    println!("cargo:rerun-if-env-changed=FREERTOS_CFLAGS");
}

fn resolve_overlay_env(default_port: &str) -> OverlayEnv {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let config_dir = manifest_dir.join("config");
    // phase-337 W5.a/W5.e — the shared config headers + section layout live in
    // the family crate; the board keeps only the numbers.
    //
    // issue 1558 — one hop is still a hop: it is right only while the CALLER
    // sits beside `nros-board-freertos`, and this function runs from four
    // different board crates. Naming the family crate from the repo root is
    // the same fact with nothing about the caller in it.
    let shared_config_dir =
        nros_build_paths::repo_root().join("packages/boards/nros-board-freertos/config");

    let freertos_dir = nros_build_paths::freertos_dir();
    let freertos_port = env::var("FREERTOS_PORT").unwrap_or_else(|_| default_port.to_string());
    let port_dir = freertos_dir.join("portable").join(&freertos_port);
    let lwip_dir = nros_build_paths::lwip_dir();
    // issue 1527 — through `env_path`, like the two lines above it: a raw
    // `env::var` skips issue 1280's three-valued rule, so a worktree build
    // resolved the kernel HERE and the config dir in the OTHER checkout.
    let freertos_config_dir =
        nros_build_paths::env_path("FREERTOS_CONFIG_DIR").unwrap_or_else(|| config_dir.clone());

    // Issue 0365 — nros-c moved to `packages/api/nros-c` in phase-321 W2.e and
    // this join was left at the old `core/nros-c`, so the emitted TU could not
    // find `<nros/app_config.h>`. Assert existence so a future move fails loud
    // here rather than deep inside `cc`.
    //
    // issue 1558 — the hop count is gone; only nros-c's own location is
    // encoded now, and the `assert!` below is what catches THAT moving. The
    // remedy 0365 left was a per-site tripwire, not a fix for the class: a
    // counted walk breaks when THIS crate moves, which is a second way to be
    // wrong that no assert here could see.
    let nros_c_include = nros_build_paths::repo_root().join("packages/api/nros-c/include");
    assert!(
        nros_c_include.join("nros/app_config.h").exists(),
        "nros-c header not at {} — did nros-c move again? (issue 0365)",
        nros_c_include.display()
    );

    OverlayEnv {
        out_dir,
        manifest_dir,
        config_dir,
        shared_config_dir,
        freertos_dir,
        port_dir,
        lwip_dir,
        freertos_config_dir,
        nros_c_include,
    }
}

/// Ask `arm-none-eabi-gcc` where `name` is for THIS board's multilib.
///
/// phase-471 W2 — three board scripts carried a byte-identical copy of this,
/// each with its own hardcoded `-mcpu` list. The flags are not a parameter: the
/// `[arch.*]` profile [`configure_cflags`] already reads IS the source, and the
/// three hardcoded copies were hand-mirrors of it —
/// `[arch.cortex-m3] cflags = ["-mcpu=cortex-m3", "-mthumb"]` and
/// `[arch.cortex-r52] cflags = ["-mcpu=cortex-r52", "-mfpu=neon-fp-armv8",
/// "-mfloat-abi=hard"]`, which is what the MPS2 copy and the other two passed
/// respectively. So this reads them from the same place the compile does.
///
/// That also closes a latent mismatch the copies had: `FREERTOS_CFLAGS` is the
/// RFC-0049 rung-1 override and wins over the profile for the actual compile,
/// but the hardcoded lists could not see it — a board pointed at a different
/// CPU through that variable compiled its C for the new one and then linked the
/// OLD one's newlib. The two answers now come from one function.
fn gcc_print_file(name: &str) -> String {
    let flags = resolve_cflags();
    let mut args: Vec<String> = flags.split_whitespace().map(str::to_string).collect();
    args.push(format!("--print-file-name={name}"));
    let out = std::process::Command::new("arm-none-eabi-gcc")
        .args(&args)
        .output()
        .expect("arm-none-eabi-gcc not found");
    let path = String::from_utf8(out.stdout)
        .expect("arm-none-eabi-gcc printed non-UTF-8")
        .trim()
        .to_string();
    // If gcc cannot resolve the file it echoes the bare name back.
    assert!(
        Path::new(&path).is_absolute(),
        "arm-none-eabi-gcc could not locate {name} for the multilib selected by `{flags}`"
    );
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_emitted_tu_carries_the_config_it_was_given() {
        let dir = tempfile::tempdir().unwrap();
        let path = emit_app_config_tu(
            dir.path(),
            &BaseConfig::default(),
            &FreertosScheduling::default(),
        );
        let tu = std::fs::read_to_string(path).unwrap();
        assert!(tu.contains(r#".locator   = "tcp/192.0.3.1:7447""#), "{tu}");
        assert!(tu.contains(".ip      = { 192, 0, 3, 10 }"), "{tu}");
        assert!(
            tu.contains(".mac     = { 0x02, 0x00, 0x00, 0x00, 0x00, 0x00 }"),
            "{tu}"
        );
        assert!(tu.contains(".prefix  = 24,"), "{tu}");
        // The number the hand-written mirror got wrong.
        assert!(tu.contains(".app_stack_bytes         = 131072u,"), "{tu}");
    }
}
