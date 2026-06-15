//! Phase 241.A (RFC-0042 D4) — **cross tier** of the merge-time platform gate.
//!
//! The host tier (`platform_header_matrix.rs`) catches the #38 capability class
//! but CANNOT see the two-libc-set class (#27/#36): that one needs the **cross
//! toolchain** (arm-none-eabi, its own newlib) plus an RTOS sysroot header on the
//! include path. A platform `.c`/`.cpp` TU then pulls TWO `<stdlib.h>`s with
//! incompatible `div_t` shapes (the RTOS's NAMED `struct div_s` vs newlib's
//! ANONYMOUS typedef) and the C++ compile dies on `conflicting declaration
//! '…div_t'`. The fix (commits `812234321`/`7b0517121`) makes the RTOS sysroot
//! win — `${RTOS}/include/cxx` prepended / SYSTEM precedence — so `<cstdlib>`
//! resolves to the RTOS wrapper and only one `div_t` exists.
//!
//! This gate reproduces the class **self-contained** (a minimal RTOS-header stub
//! under `fixtures/cross_libc_precedence/`, no RTOS submodule) so it is cheap and
//! runs anywhere the cross toolchain is provisioned (`just nuttx setup` / the SDK
//! `arm-none-eabi-gcc`). It is a RELATIVE assertion, robust to toolchain version:
//!   * compile the probe with the RTOS sysroot NOT winning `<cstdlib>` (plain
//!     `-I`). If it compiles anyway, this toolchain's newlib `div_t` does not
//!     conflict → the class is not reproducible here → **skip**.
//!   * if it clashes (the class IS live), compile with the RTOS `include/cxx`
//!     prepended (the fix). That MUST compile — else the include-precedence wiring
//!     that keeps the RTOS sysroot winning has regressed (#27/#36 back on main).
//!
//! So the gate goes red exactly when a PR reintroduces the two-libc precedence
//! bug, on the PR — not days later in an on-demand e2e build.

use std::{path::PathBuf, process::Command};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cross_libc_precedence")
}

/// Locate the cross C++ compiler. Prefer the provisioned SDK toolchain (the one
/// the e2e/nuttx build uses — `~/.nros/sdk/arm-none-eabi-gcc/<ver>/bin`), else
/// fall back to `arm-none-eabi-g++` on PATH (the activate-wired SDK bin).
fn cross_gxx() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("HOME") {
        let sdk = PathBuf::from(home).join(".nros/sdk/arm-none-eabi-gcc");
        if let Ok(rd) = std::fs::read_dir(&sdk) {
            for e in rd.flatten() {
                let bin = e.path().join("bin/arm-none-eabi-g++");
                if bin.is_file() {
                    return Some(bin);
                }
            }
        }
    }
    // PATH fallback — confirm it runs.
    if Command::new("arm-none-eabi-g++")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Some(PathBuf::from("arm-none-eabi-g++"));
    }
    None
}

/// Does this cross g++ ship a usable libstdc++ (the C++ standard headers the
/// probe pulls)? Some bare-metal `arm-none-eabi` toolchains provision only the
/// newlib C library — no `<type_traits>`/`<cstdlib>`. That is an unsuitable
/// toolchain for this gate (an unmet precondition), NOT the #27/#36 clash, so
/// the caller must `skip!` rather than report a false `div_t`-gate failure.
fn cxx_stdlib_available(gxx: &PathBuf) -> bool {
    use std::io::Write;
    let Ok(dir) = tempfile::tempdir() else {
        return false;
    };
    let src = dir.path().join("cap.cpp");
    let Ok(mut f) = std::fs::File::create(&src) else {
        return false;
    };
    if f.write_all(b"#include <type_traits>\n#include <cstdlib>\nint main(){return 0;}\n")
        .is_err()
    {
        return false;
    }
    Command::new(gxx)
        .args(["-std=c++17", "-fno-exceptions", "-c"])
        .arg(&src)
        .arg("-o")
        .arg("/dev/null")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Compile the probe. `rtos_cxx_first` = the #27/#36 fix (RTOS `include/cxx`
/// prepended so `<cstdlib>` resolves to the RTOS wrapper). Returns (ok, output).
fn compile(gxx: &PathBuf, rtos_cxx_first: bool) -> (bool, String) {
    let fix = fixture_dir();
    let stub_inc = fix.join("rtos-stub/include");
    let mut cmd = Command::new(gxx);
    cmd.arg("-std=c++17").arg("-fno-exceptions");
    if rtos_cxx_first {
        cmd.arg("-I").arg(stub_inc.join("cxx"));
    }
    cmd.arg("-I").arg(&stub_inc);
    cmd.arg("-c")
        .arg(fix.join("probe.cpp"))
        .arg("-o")
        .arg("/dev/null");
    let out = cmd.output().expect("spawn cross g++");
    let log = String::from_utf8_lossy(&out.stderr).into_owned();
    (out.status.success(), log)
}

#[test]
fn cross_libc_two_set_precedence_holds() {
    let Some(gxx) = cross_gxx() else {
        nros_tests::skip!(
            "cross toolchain arm-none-eabi-g++ not provisioned — run `just nuttx setup` \
             (the #27/#36 two-libc gate needs the cross newlib)"
        );
    };

    // 0. Toolchain capability: the probe needs libstdc++ (`<type_traits>` /
    //    `<cstdlib>`). A C-only newlib cross can't compile it — that is an
    //    unmet precondition, not the #27/#36 clash. Skip rather than false-fail.
    if !cxx_stdlib_available(&gxx) {
        nros_tests::skip!(
            "cross toolchain ({}) has no usable libstdc++ (`<type_traits>`/`<cstdlib>` \
             absent) — the #27/#36 two-libc gate needs a C++-capable newlib cross",
            gxx.display()
        );
    }

    // 1. Broken precedence (RTOS sysroot reachable but not winning <cstdlib>).
    let (broken_ok, broken_log) = compile(&gxx, false);
    if broken_ok {
        nros_tests::skip!(
            "cross toolchain ({}) newlib `div_t` does not conflict with the RTOS-shape \
             decl — the #27/#36 two-libc class is not reproducible on this toolchain; \
             nothing to gate",
            gxx.display()
        );
    }
    // Sanity: the failure must be the div_t clash we model, not an unrelated error
    // (a broken stub/probe would falsely "pass" the negative direction).
    assert!(
        broken_log.contains("div_t") && broken_log.to_lowercase().contains("conflict"),
        "broken-precedence compile failed for a reason OTHER than the modelled div_t \
         clash — fix the gate fixture, do not assume the precedence bug:\n{broken_log}"
    );

    // 2. With the RTOS `include/cxx` prepended (the #27/#36 fix), the SAME probe
    //    MUST compile — that is the invariant the platform build wiring upholds.
    let (fixed_ok, fixed_log) = compile(&gxx, true);
    assert!(
        fixed_ok,
        "phase-241.A cross gate: the RTOS-cxx-first include precedence no longer clears \
         the #27/#36 two-libc `div_t` clash — the SYSTEM/`include/cxx` precedence that \
         keeps the RTOS sysroot winning has regressed (see nuttx_ffi_build.rs / the NuttX \
         NanoRos cmake SYSTEM include):\n{fixed_log}"
    );
}
