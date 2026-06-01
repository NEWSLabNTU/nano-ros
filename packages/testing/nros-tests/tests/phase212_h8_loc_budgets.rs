//! Phase 212.H.8 — LoC budget gates.
//!
//! Phase 212 §Acceptance freezes two hard LoC budgets (the third —
//! `nros-build` `src/` ≤ 550 LoC — was retired with Phase 212.C):
//!
//!   * each RTOS adapter shim ≤ 200 LoC
//!   * cmake `nano_ros_workspace_metadata()` ≤ 150 LoC
//!
//! Each gate calls the `tokei` binary and asserts on the `code` count —
//! comments + blanks do not count toward the budget. The `tokei` binary
//! is the canonical measurement tool the Phase 212 docs name
//! explicitly, so we shell out rather than pulling a fresh crate dep.
//!
//! Skip discipline: `tokei` IS the measurement, so if it is missing the
//! gate cannot run; we `nros_tests::skip!` (which panics with the
//! `[SKIPPED]` prefix the CI runner recognises). Every other failure
//! path is a hard `assert!` — silent early-return is forbidden.
//!
//! ## Adapter shim path mapping (verified 2026-06-01)
//!
//! | RTOS       | Shim path                                              |
//! |------------|--------------------------------------------------------|
//! | Zephyr     | `zephyr/cmake/nros_system_generate.cmake`              |
//! | NuttX      | `integrations/nuttx/` (dir sum — Makefile/CMake/glue)  |
//! | ThreadX    | `cmake/NanoRosThreadxSystemCodegen.cmake`              |
//! | ESP-IDF    | `integrations/nano-ros/CMakeLists.txt` (esp-idf component) |
//! | PlatformIO | `integrations/platformio/nros_codegen.py` (extra_script) |
//! | PX4        | `integrations/px4/module-template/` (dir sum)          |
//! | FreeRTOS   | SKIPPED — H.3 makes the cargo path itself the adapter; |
//! |            | no separate shim file ships in-tree (per-board BSP     |
//! |            | crate's `build.rs` carries the integration).           |
//!
//! When you re-home one of these, update the table AND the `SHIMS`
//! const below — the test reads the table for its assertion loop.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use nros_tests::project_root;

const BUDGET_WORKSPACE_METADATA: u64 = 150;
const BUDGET_ADAPTER_SHIM: u64 = 200;

/// Adapter shim entries: `(label, repo-relative path)`. A path may name
/// a single file (Zephyr / ThreadX / ESP-IDF / PlatformIO) or a
/// directory (NuttX / PX4) — in the directory case we sum the `code`
/// counts across all languages tokei recognises inside it.
const SHIMS: &[(&str, &str)] = &[
    ("zephyr", "zephyr/cmake/nros_system_generate.cmake"),
    ("nuttx", "integrations/nuttx"),
    ("threadx", "cmake/NanoRosThreadxSystemCodegen.cmake"),
    ("esp-idf", "integrations/nano-ros/CMakeLists.txt"),
    ("platformio", "integrations/platformio/nros_codegen.py"),
    ("px4", "integrations/px4/module-template"),
];

/// Returns the `code` LoC count under `path`, summed across every
/// language tokei recognises. `path` may be a single file or a
/// directory; tokei recurses on dirs.
///
/// `lang_filter` optionally restricts to one language (e.g. `"Rust"`)
/// for single-language gates. `None` sums across all languages — used
/// for adapter shims so a mixed Makefile / CMake / Kconfig directory
/// still totals one budget.
fn tokei_code_loc(path: &Path, lang_filter: Option<&str>) -> u64 {
    assert!(
        path.exists(),
        "tokei target missing — adapter shim moved? update the SHIMS table in \
         phase212_h8_loc_budgets.rs. Missing path: {}",
        path.display()
    );

    let mut cmd = Command::new("tokei");
    cmd.arg("--output").arg("json");
    if let Some(lang) = lang_filter {
        cmd.arg("--types").arg(lang);
    }
    cmd.arg(path);

    let out = match cmd.stderr(Stdio::piped()).output() {
        Ok(o) => o,
        Err(e) => nros_tests::skip!("tokei not on PATH ({e}) — install via `cargo install tokei`"),
    };
    assert!(
        out.status.success(),
        "tokei failed on {}: {}",
        path.display(),
        String::from_utf8_lossy(&out.stderr)
    );

    let parsed: serde_json::Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("tokei JSON parse failed on {}: {e}", path.display()));

    // Prefer the top-level `Total.code` aggregate — present whenever
    // tokei finds at least one recognised file. Falls back to summing
    // the per-language `code` fields (tokei has historically emitted
    // both shapes; the manual sum keeps us robust).
    if let Some(code) = parsed
        .get("Total")
        .and_then(|t| t.get("code"))
        .and_then(|c| c.as_u64())
    {
        return code;
    }

    let mut total: u64 = 0;
    let obj = parsed
        .as_object()
        .unwrap_or_else(|| panic!("tokei JSON not an object for {}", path.display()));
    for (key, val) in obj {
        if key == "Total" {
            continue;
        }
        if let Some(code) = val.get("code").and_then(|c| c.as_u64()) {
            total += code;
        }
    }
    total
}

#[test]
fn cmake_workspace_metadata_under_150_loc() {
    let file = project_root().join("cmake/nano_ros_workspace_metadata.cmake");
    // Single file — tokei still emits Total.code, no language filter
    // needed (CMake is the only language inside).
    let code = tokei_code_loc(&file, None);
    assert!(
        code <= BUDGET_WORKSPACE_METADATA,
        "Phase 212 acceptance budget violated: \
         cmake/nano_ros_workspace_metadata.cmake = {code} LoC \
         > {BUDGET_WORKSPACE_METADATA}."
    );
    eprintln!("[OK] nano_ros_workspace_metadata.cmake = {code} / {BUDGET_WORKSPACE_METADATA} LoC");
}

#[test]
fn rtos_adapter_shims_under_200_loc_each() {
    let root = project_root();
    let mut violations: Vec<String> = Vec::new();
    let mut reports: Vec<(String, u64)> = Vec::new();

    for (label, rel) in SHIMS {
        let abs: PathBuf = root.join(rel);
        // Sum across every language tokei recognises — adapter shim
        // dirs (nuttx, px4) mix CMake + Makefile + Kconfig + C++ +
        // Python, and the 200-LoC budget covers them as one unit.
        let code = tokei_code_loc(&abs, None);
        reports.push((label.to_string(), code));
        if code > BUDGET_ADAPTER_SHIM {
            violations.push(format!(
                "  - {label} ({rel}): {code} LoC > {BUDGET_ADAPTER_SHIM}"
            ));
        }
    }

    for (label, code) in &reports {
        eprintln!("[OK] adapter shim {label} = {code} / {BUDGET_ADAPTER_SHIM} LoC");
    }

    assert!(
        violations.is_empty(),
        "Phase 212.H.8 adapter-shim 200-LoC budget violated:\n{}",
        violations.join("\n")
    );
}
