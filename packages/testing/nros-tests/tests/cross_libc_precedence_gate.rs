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
    // Sanity: the failure must be the two-libc clash we model, not an unrelated
    // error (a broken stub/probe would falsely "pass" the negative direction).
    assert!(
        models_two_libc_clash(&broken_log),
        "broken-precedence compile failed for a reason OTHER than the modelled \
         two-libc clash — fix the gate fixture, do not assume the precedence \
         bug:\n{broken_log}"
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

/// Does this compile log show the RTOS `stdlib.h` winning over the cross
/// newlib's — the #27/#36 two-libc clash — rather than some unrelated error?
///
/// Issue 0995. It has TWO manifestations, and which one you get depends on
/// which cross toolchain is installed:
///
///   * SDK store (`~/.nros/sdk/arm-none-eabi-gcc/13.2-nros1`, newlib 13.2.1):
///     newlib's own `stdlib.h` is reached FIRST, so the stub's is a
///     redefinition —
///     error: conflicting declaration 'typedef struct div_s div_t'
///
///   * the CI container's apt cross (newlib 10.3.1): the stub's `stdlib.h` is
///     reached INSTEAD of newlib's, so newlib's `<cstdlib>` finds nothing to
///     re-export —
///     /usr/include/newlib/c++/10.3.1/bits/std_abs.h:52:11:
///     error: 'abs' has not been declared in '::'
///
/// Both are the stub shadowing the real libc; only the first was modelled, so
/// the gate failed on the container with "fix the gate fixture" — correctly
/// refusing to conclude, and correctly telling us the fixture was the problem.
fn models_two_libc_clash(log: &str) -> bool {
    let lower = log.to_lowercase();
    // Manifestation 1: a redefinition, naming the type the stub redeclares.
    if lower.contains("div_t") && lower.contains("conflict") {
        return true;
    }
    // Manifestation 2: the C++ `<cstdlib>` chain cannot find the C names it
    // re-exports, because the stub's header replaced the one that declares
    // them. Keyed on BOTH halves so an unrelated "not declared" elsewhere does
    // not qualify.
    let from_cstdlib_chain = lower.contains("cstdlib") || lower.contains("std_abs.h");
    let missing_c_names = lower.contains("has not been declared in");
    from_cstdlib_chain && missing_c_names
}

#[test]
fn the_clash_predicate_accepts_both_toolchains_and_rejects_noise() {
    // Issue 0995 — REAL logs, not paraphrases: the first from this host's SDK
    // cross, the second copied from the CI run that failed (33654481082).
    let sdk_cross = "\
rtos-stub/include/stdlib.h:19:23: error: conflicting declaration 'typedef struct div_s div_t'
   19 | typedef struct div_s  div_t;
.../c++/13.2.1/cstdlib:79: note: previous declaration as 'typedef struct div_t div_t'";
    assert!(
        models_two_libc_clash(sdk_cross),
        "the div_t redefinition is the originally modelled manifestation"
    );

    let apt_cross = "\
In file included from /usr/include/newlib/c++/10.3.1/cstdlib:77,
                 from .../cross_libc_precedence/probe.cpp:9:
/usr/include/newlib/c++/10.3.1/bits/std_abs.h:52:11: error: 'abs' has not been declared in '::'
   52 |   using ::abs;";
    assert!(
        models_two_libc_clash(apt_cross),
        "the container's newlib shadows the other way and must also qualify"
    );

    // A genuinely unrelated failure must still fail the gate — that is the
    // whole point of the sanity check.
    assert!(
        !models_two_libc_clash("probe.cpp:3:10: fatal error: nowhere.h: No such file or directory"),
        "an unrelated error must NOT be read as the clash"
    );
    assert!(
        !models_two_libc_clash("error: 'frobnicate' has not been declared in '::'"),
        "a `not declared` outside the cstdlib chain must NOT qualify"
    );
}
