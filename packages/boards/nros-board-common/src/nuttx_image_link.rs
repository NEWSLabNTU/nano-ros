//! #127 — shared NuttX flat-build image link (the board-centric entry-link
//! convention, RFC-0032 "third leg").
//!
//! NuttX uses a flat-build model: the cargo binary IS the kernel image. This
//! helper stages the *dynamic* link inputs in the BOARD crate's build script
//! and emits the PROPAGATING directives (`cargo:rustc-link-search` /
//! `cargo:rustc-link-lib` — which, unlike `cargo:rustc-link-arg`, propagate
//! from a dependency's build script to the final `[[bin]]` link), so a
//! dependent Entry pkg links a bootable image with ZERO build.rs of its own:
//!
//! 1. cpp-preprocess the board linker script (it carries directives needing
//!    `<nuttx/config.h>`) → `OUT_DIR/<script name>` — found by GNU ld's
//!    `-T<name>` by-name lookup through the propagated `-L OUT_DIR`;
//! 2. compile the board's builtins stub (empty `g_builtins` table — blocks
//!    libapps' contaminated `builtin_list.o`; see the stub's header in
//!    `nros-board-nuttx-qemu/c/nuttx_builtins_stub.c`);
//! 3. archive the arch vector-table object + the stub into
//!    `libnros_nuttx_boot.a` and link it `-bundle,+whole-archive` (both
//!    members always pulled; the ld script's section placement pins the
//!    vector table, not object order);
//! 4. emit link-search for `OUT_DIR` + `$NUTTX_DIR/staging` + the board lib
//!    dir.
//!
//! The *static* args (`-T<script>`, `--entry=__start`, `-nostartfiles`,
//! `-nodefaultlibs`, the kernel-lib `--start-group` list, `-lgcc`) stay in
//! the Entry pkg's `.cargo/config.toml` rustflags, rendered from the board
//! descriptor's `cargo_config` (nros-board.toml). The cpu link-args there
//! select the gcc driver's multilib, so the trailing `-lgcc` resolves the
//! ARM intrinsics without an absolute `-print-libgcc-file-name` path.
//!
//! Arch-specifics come from the `NUTTX_*` env family (the board overlay sets
//! them); defaults are the qemu-arm cortex-a7 hardfloat values, mirroring
//! [`crate::nuttx_platform_build::run_platform`].
//!
//! Env (all optional, arm defaults):
//! - `NUTTX_CROSS` — cross C compiler driver (default `arm-none-eabi-gcc`);
//!   the archiver is derived by swapping the trailing `gcc` for `ar`.
//! - `NUTTX_PLATFORM_CFLAGS` — arch flags for the stub compile (default
//!   `-mcpu=cortex-a7 -mfloat-abi=hard -mfpu=neon-vfpv4`).
//! - `NUTTX_ARCH_INCLUDES` — space-separated include dirs relative to
//!   `NUTTX_DIR` (default `arch/arm/src/{chip,common,armv7-a}`). Same var
//!   the platform/FFI helpers read, so a board sets it once.
//! - `NUTTX_LD_SCRIPT` — board linker script relative to `NUTTX_DIR`
//!   (default `boards/arm/qemu/qemu-armv7a/scripts/dramboot.ld`).
//! - `NUTTX_VECTORTAB` — arch vector-table object relative to `NUTTX_DIR`
//!   (default `arch/arm/src/arm_vectortab.o`).
//! - `NUTTX_BOARD_LIB_DIR` — `libboard.a` dir relative to `NUTTX_DIR`
//!   (default `arch/arm/src/board`).
//!
//! Gated on `NUTTX_DIR` (env absent → plain host `cargo check` still works)
//! and on `staging/libc.a` (tree provisioned by `just nuttx build`).

use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

/// Stage the NuttX image-link inputs and emit the propagating directives.
///
/// `builtins_stub` is the calling board crate's empty-builtins C stub (kept
/// in the board crate's `c/`, next to the board that owns the entry link).
pub fn run_image_link(builtins_stub: &Path) {
    // issue 0491 — `NUTTX_DIR` names a DIRECTORY, and cargo compares an env
    // value as TEXT, so fingerprinting the spelling lets two consumers that
    // spell one directory differently invalidate each other inside a shared
    // `--target-dir`. Not replaced by a content watch: NuttX is BUILT IN
    // PLACE, so watching its tree would leave this permanently dirty after
    // every kernel build. The specific inputs are declared per file.
    println!("cargo:rerun-if-env-changed=NUTTX_CROSS");
    println!("cargo:rerun-if-env-changed=NUTTX_PLATFORM_CFLAGS");
    println!("cargo:rerun-if-env-changed=NUTTX_ARCH_INCLUDES");
    println!("cargo:rerun-if-env-changed=NUTTX_LD_SCRIPT");
    println!("cargo:rerun-if-env-changed=NUTTX_VECTORTAB");
    println!("cargo:rerun-if-env-changed=NUTTX_BOARD_LIB_DIR");

    // Strictly env-gated (NOT the nros-build-paths repo fallback): the image
    // link only makes sense inside a provisioned fixture/example build, which
    // always exports NUTTX_DIR. A host `cargo check` of a dependent Entry pkg
    // must stay link-directive-free.
    let nuttx_dir = match env::var("NUTTX_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => return,
    };

    let nuttx_cross = env::var("NUTTX_CROSS").unwrap_or_else(|_| "arm-none-eabi-gcc".to_string());
    let nuttx_ar = nuttx_cross
        .strip_suffix("gcc")
        .map(|prefix| format!("{prefix}ar"))
        .unwrap_or_else(|| "ar".to_string());
    let cflags: Vec<String> = env::var("NUTTX_PLATFORM_CFLAGS")
        .unwrap_or_else(|_| "-mcpu=cortex-a7 -mfloat-abi=hard -mfpu=neon-vfpv4".to_string())
        .split_whitespace()
        .map(String::from)
        .collect();
    let arch_includes: Vec<String> = env::var("NUTTX_ARCH_INCLUDES")
        .unwrap_or_else(|_| {
            "arch/arm/src/chip arch/arm/src/common arch/arm/src/armv7-a".to_string()
        })
        .split_whitespace()
        .map(String::from)
        .collect();
    let ld_script_rel = env::var("NUTTX_LD_SCRIPT")
        .unwrap_or_else(|_| "boards/arm/qemu/qemu-armv7a/scripts/dramboot.ld".to_string());
    // Phase-285 W4 — an EMPTY `NUTTX_VECTORTAB` means "this arch has no
    // vector-table head object" (riscv rv-virt: the reset path lives in the
    // kernel libs; only arm needs `arm_vectortab.o` at the archive head).
    let vectortab_rel =
        env::var("NUTTX_VECTORTAB").unwrap_or_else(|_| "arch/arm/src/arm_vectortab.o".to_string());
    let board_lib_rel =
        env::var("NUTTX_BOARD_LIB_DIR").unwrap_or_else(|_| "arch/arm/src/board".to_string());

    // phase-339 W2 — resolve out of this arch's export SNAPSHOT when there is
    // one, so the shared live tree cannot invalidate an already-linked image
    // (issue 0433). Each path falls back to its live-tree spelling, which keeps
    // a pre-phase-339 tree working and lets the migration land one arch at a
    // time.
    let kernel = crate::nuttx_export::kernel_libs(&nuttx_dir);
    let staging = kernel.libs.clone();
    // The snapshot flattens these: the linker script lands in `scripts/` and the
    // vector table is copied into `startup/` by `build-nuttx.sh` (it is an
    // intermediate object `make export` does not ship).
    let linker_script = crate::nuttx_export::snapshot_or_tree(
        &kernel,
        &nuttx_dir,
        &format!(
            "scripts/{}",
            std::path::Path::new(&ld_script_rel)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        ),
        &ld_script_rel,
    );
    let vectortab = (!vectortab_rel.is_empty()).then(|| {
        crate::nuttx_export::snapshot_or_tree(
            &kernel,
            &nuttx_dir,
            &format!(
                "startup/{}",
                std::path::Path::new(&vectortab_rel)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            ),
            &vectortab_rel,
        )
    });
    // Phase-285 W5 — a CONFIGURED vectortab that does not exist means this
    // build is not the image-link lane for this arch (e.g. the riscv C lane
    // compiles the riscv board crate with the helper's arm DEFAULT path
    // against an rv-virt tree, and the C kernel link never consumes the boot
    // archive). Skip the image link gracefully instead of failing `ar` on a
    // missing member; the rust Entry lane sets the arch-correct env.
    if let Some(vt) = &vectortab
        && !vt.exists()
    {
        println!(
            "cargo:warning=nuttx_image_link: vectortab {} absent — skipping image-link staging",
            vt.display()
        );
        return;
    }
    // issue 0456 — the accommodation above tolerates the WRONG ARCH as long as
    // the file happens to be missing, and that is the only thing that was
    // stopping an arm vector table from reaching a riscv image. Once both
    // arches build in one lane, `arch/arm/src/arm_vectortab.o` is simply always
    // present in the shared in-tree checkout, `exists()` is true, and `ar`
    // archives it — `ar` does not check machine types. The link then fails
    // `cannot find -lnros_nuttx_boot`, because `ld` skips an incompatible
    // archive and then looks no further, so the diagnostic names the wrong
    // problem entirely (a missing file) three steps from the cause.
    //
    // Check it here, where both facts are in hand.
    if let Some(vt) = &vectortab {
        assert_vectortab_arch(vt);
    }
    let board_lib_dir = nuttx_dir.join(&board_lib_rel);
    println!(
        "cargo:rerun-if-changed={}",
        staging.join("libc.a").display()
    );
    if let Some(vt) = &vectortab {
        println!("cargo:rerun-if-changed={}", vt.display());
    }
    println!("cargo:rerun-if-changed={}", linker_script.display());
    println!("cargo:rerun-if-changed={}", builtins_stub.display());
    // Issue 0511 — the config header IS the memory map (it supplies
    // CONFIG_FLASH_*/CONFIG_RAM_* to the cpp pass below), so a reconfigure must
    // invalidate this artifact. Watch BOTH spellings whether or not they exist,
    // for 0477's reason: an edge emitted only on the path that won leaves the
    // artifact valid forever when the other one later appears.
    println!(
        "cargo:rerun-if-changed={}",
        nuttx_dir.join("include/nuttx/config.h").display()
    );
    if !staging.join("libc.a").exists() {
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR set by cargo for build.rs"));

    // Shared -isystem/-I set for the preprocess + stub compile (both need
    // <nuttx/config.h> and the arch headers, like the NuttX build itself).
    //
    // Issue 0511 — the HEADERS must come from the same per-arch snapshot the
    // linker script above does. `$NUTTX_DIR/include/nuttx/config.h` belongs to
    // whichever arch the SHARED tree was configured for LAST, and this script is
    // `cpp`-preprocessed, so those macros ARE the memory map:
    //
    //     MEMORY { ROM (rx) : ORIGIN = CONFIG_FLASH_START, LENGTH = CONFIG_FLASH_SIZE
    //              RAM (rwx): ORIGIN = CONFIG_RAM_START,   LENGTH = CONFIG_RAM_SIZE }
    //
    // Build riscv after arm — which `lane=tier2` does — and the ARM image is
    // linked with the RISC-V map: `CONFIG_FLASH_SIZE` is 0 there, so ROM has
    // LENGTH = 0 and every byte placed in it "overflows". That is the whole of
    // 0511's `region ROM overflowed by N bytes`: N was never an excess, it was
    // the image's ROM-placed size against a zero-length region, which is why it
    // stayed constant across revisions and survived clean rebuilds (the stale
    // `.config` lives in the submodule, not in any target dir).
    //
    // phase-339 W2 made the export per-arch and moved the linker script onto
    // it; the include path was left on the shared tree, so half the inputs came
    // from the snapshot and half from whatever was configured last.
    let snapshot_include = crate::nuttx_export::snapshot_root(&nuttx_dir)
        .map(|root| root.join("include"))
        .filter(|p| p.join("nuttx/config.h").is_file());
    let include_root = snapshot_include
        .clone()
        .unwrap_or_else(|| nuttx_dir.join("include"));
    if let Some(inc) = &snapshot_include {
        println!(
            "cargo:rerun-if-changed={}",
            inc.join("nuttx/config.h").display()
        );
    }
    let include_args = |cmd: &mut Command| {
        cmd.arg(format!("-isystem{}", include_root.display()));
        for inc in &arch_includes {
            cmd.arg(format!("-I{}", nuttx_dir.join(inc).display()));
        }
        cmd.arg(format!("-I{}", nuttx_dir.join("sched").display()));
    };

    // (1) Preprocess the flat-build linker script → OUT_DIR/<name>. The
    // Entry config's `-T<name>` resolves it by name through the propagated
    // `-L OUT_DIR` (GNU ld searches -L dirs for -T scripts).
    let script_name = linker_script
        .file_name()
        .expect("linker script path has a file name");
    let processed_ld = out_dir.join(script_name);
    let mut preprocess = Command::new(&nuttx_cross);
    preprocess.args(["-E", "-P", "-x", "c", "-D__NuttX__", "-D__KERNEL__"]);
    include_args(&mut preprocess);
    preprocess.arg(&linker_script).arg("-o").arg(&processed_ld);
    let status = preprocess.status().unwrap_or_else(|e| {
        panic!("failed to preprocess NuttX linker script ({nuttx_cross}): {e}")
    });
    assert!(status.success(), "NuttX linker script preprocessing failed");

    // (2) Empty builtins table — see the stub's header.
    let stub_obj = out_dir.join("nuttx_builtins_stub.o");
    let mut stub_cc = Command::new(&nuttx_cross);
    stub_cc.arg("-c");
    for f in &cflags {
        stub_cc.arg(f);
    }
    stub_cc.args(["-std=c11", "-D__NuttX__", "-D__KERNEL__"]);
    include_args(&mut stub_cc);
    stub_cc.arg(builtins_stub).arg("-o").arg(&stub_obj);
    let stub_status = stub_cc
        .status()
        .unwrap_or_else(|e| panic!("failed to compile NuttX builtins stub ({nuttx_cross}): {e}"));
    assert!(stub_status.success(), "NuttX builtins stub compile failed");

    // (3) Boot archive: vectortab (reset path head object) + builtins stub.
    let boot_lib = out_dir.join("libnros_nuttx_boot.a");
    let _ = std::fs::remove_file(&boot_lib);
    let mut ar_cmd = Command::new(&nuttx_ar);
    ar_cmd.arg("crs").arg(&boot_lib);
    if let Some(vt) = &vectortab {
        ar_cmd.arg(vt);
    }
    let ar_status = ar_cmd
        .arg(&stub_obj)
        .status()
        .unwrap_or_else(|e| panic!("failed to archive libnros_nuttx_boot.a ({nuttx_ar}): {e}"));
    assert!(ar_status.success(), "libnros_nuttx_boot.a archive failed");

    // (4) The propagating directives. `-bundle` keeps the archive standalone
    // in OUT_DIR (bundling into the rlib is incompatible with
    // `+whole-archive` — same constraint `run_platform` documents,
    // issue-0048) and puts the `-l` at the FINAL binary link, BEFORE the
    // `.cargo/config.toml` trailing `-C link-arg` kernel group — so the
    // builtins stub preempts `-lapps`' contaminated `builtin_list.o`, and
    // the board rlib's `nsh_main` (earlier still, in the rlib list) preempts
    // NSH's.
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-search=native={}", staging.display());
    println!("cargo:rustc-link-search=native={}", board_lib_dir.display());
    println!("cargo:rustc-link-lib=static:-bundle,+whole-archive=nros_nuttx_boot");
}

/// Fail if `vectortab` is not built for the arch this crate is compiling for.
///
/// issue 0456. Reads the ELF header's `e_machine` directly rather than shelling
/// out to `readelf`: the check must work in a build script on any host, and the
/// two bytes it needs are at a fixed offset in every ELF ever written.
///
/// Unreadable or non-ELF input is NOT an error here — `ar` and `ld` will say so
/// far better than a guess would, and a build script that refuses to run on an
/// input it merely failed to parse is worse than the bug.
fn assert_vectortab_arch(vectortab: &Path) {
    let Ok(target_arch) = env::var("CARGO_CFG_TARGET_ARCH") else {
        return;
    };
    let Some(want) = elf_machine_for(&target_arch) else {
        return; // an arch this helper has not been taught; do not guess
    };
    let Ok(bytes) = std::fs::read(vectortab) else {
        return;
    };
    // e_ident[EI_MAG] = 0x7f 'E' 'L' 'F'; e_machine is a u16 at offset 18,
    // in the file's own endianness (e_ident[EI_DATA] at offset 5: 1 = little).
    if bytes.len() < 20 || &bytes[0..4] != b"\x7fELF" {
        return;
    }
    let got = if bytes[5] == 2 {
        u16::from_be_bytes([bytes[18], bytes[19]])
    } else {
        u16::from_le_bytes([bytes[18], bytes[19]])
    };
    assert!(
        got == want,
        "nuttx_image_link: vector table {} is ELF machine {:#x}, but this build \
         targets {} (machine {:#x}).\n  \
         The boot archive would be rejected by the linker as incompatible, and \
         reported as a MISSING library (`cannot find -lnros_nuttx_boot`).\n  \
         A riscv recipe must `source scripts/nuttx/riscv-env.sh` — without it \
         the board helpers take their qemu-arm defaults, including \
         NUTTX_VECTORTAB (issue 0456).",
        vectortab.display(),
        got,
        target_arch,
        want,
    );
}

/// `e_machine` for the target arches these boards build for.
fn elf_machine_for(target_arch: &str) -> Option<u16> {
    match target_arch {
        "arm" => Some(0x28),                       // EM_ARM
        "aarch64" => Some(0xb7),                   // EM_AARCH64
        a if a.starts_with("riscv") => Some(0xf3), // EM_RISCV
        _ => None,
    }
}
