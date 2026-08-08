//! phase-329 W5 — the negative-diagnostic registry + its enforcement gate.
//!
//! ## The rule (AGENTS.md E1 / issue 0196)
//!
//! "No compilation inside tests." A test's build/compile/link belongs in the
//! FIXTURE BUILD stage; the test consumes the prebuilt artifact. The sanctioned
//! exception is a FAIL-PATH diagnostic — a configure/compile/link that MUST FAIL
//! (or, for a few, a build whose repetition/sandbox IS the assertion) cannot be a
//! passing prebuilt fixture.
//!
//! This module is the explicit allowlist of every test file permitted to invoke a
//! compiler/build tool (`cargo build|check` / `cmake` / `cc` / `gcc` / `g++` /
//! `make`) at RUNTIME, each with the tool it invokes and why it cannot be
//! prebuilt. `enforce_registry` scans `tests/` for such invocations and FAILS on
//! any file NOT listed here — the 0196 rule: the gate covers the CLASS, so a new
//! compile-at-test can't slip in unsanctioned. A file that is converted to a
//! build-stage fixture stops matching the scan and is removed from the list.
//!
//! (Variable-held compilers — e.g. `cross_libc_precedence_gate`'s
//! `Command::new(&gxx)` — are not detected by the literal scan; they are listed
//! here explicitly so they remain sanctioned and auditable.)

use std::{collections::BTreeSet, fs, path::PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A configure/compile/link that MUST FAIL — a passing prebuilt would defeat
    /// the assertion.
    FailPath,
    /// A positive build that structurally cannot be a cached prebuilt artifact
    /// (its repetition/sandbox/dry-run IS the point, or the composite has no
    /// single compile_check builder yet).
    RuntimeException,
}

struct Entry {
    file: &'static str,
    kind: Kind,
    tool: &'static str,
    reason: &'static str,
}

/// The registry. Adding a runtime compile/build to a test means adding a row here
/// (or, preferably, moving the build to the fixture stage so no row is needed).
const REGISTRY: &[Entry] = &[
    // phase-340 W3 / issue 0482 sweep — added 2026-08-08. The post-merge sweep
    // caught this; `check-fast` does not run `enforce_registry`, so the gate's
    // own author could not have seen it from a buildless run.
    Entry {
        file: "cargo_target_spelling.sh",
        kind: Kind::RuntimeException,
        tool: "cmake (configure only)",
        reason: "asserts every cargo command cmake EMITS carries one `--target` spelling. \
                 It configures four synthetic scopes (no Corrosion; Corrosion in a parent \
                 scope; a cross toolchain; nothing readable) and greps the generated \
                 command lines — configure-only, it compiles nothing. The four scopes ARE \
                 the assertion, so there is no single prebuilt that could stand in: a \
                 cached artifact would prove the resolver ran once, not that it resolves \
                 the same way in every scope cmake presents",
    },
    // ---- FAIL-PATH diagnostics (must fail; can't be a passing prebuilt) ----
    Entry {
        file: "diagnostic_verbatim.rs",
        kind: Kind::FailPath,
        tool: "cargo check + cmake",
        reason: "asserts a rustc E0432 and a cmake package-not-found diagnostic reach the \
                 terminal VERBATIM — both fixtures must FAIL to build/configure",
    },
    Entry {
        file: "cmake_node_register_misuse.rs",
        kind: Kind::FailPath,
        tool: "cmake",
        reason: "asserts nano_ros_node_register/entry FATAL_ERRORs at configure on an \
                 unqualified class / embedded deploy (RFC-0057 D2) — a must-fail configure",
    },
    Entry {
        file: "cmake_platform_matrix.rs",
        kind: Kind::FailPath,
        tool: "cmake",
        reason: "asserts a missing NANO_ROS_BOARD FATAL_ERRORs at configure — a must-fail configure",
    },
    Entry {
        file: "native_main_macro_misuse.rs",
        kind: Kind::FailPath,
        tool: "cargo check",
        reason: "asserts nros::main! misuse (custom_tasks outside RTIC, unknown board) fails \
                 `cargo check` with a specific compile diagnostic — must-fail",
    },
    Entry {
        file: "native_orchestration_misuse.rs",
        kind: Kind::FailPath,
        tool: "cargo check",
        reason: "asserts the removed `launch=` arm fails `cargo check` with a removal \
                 diagnostic (RFC-0032 §7) — must-fail",
    },
    Entry {
        file: "zpico_drift_gate.rs",
        kind: Kind::FailPath,
        tool: "cargo build",
        reason: "a corrupted platform tree must PANIC the zpico-sys build script (drift \
                 sentinel); the pristine round-trip needs the NROS_PLATFORMS_DIR sandbox \
                 injected at configure, so neither half is a static artifact",
    },
    Entry {
        file: "cross_libc_precedence_gate.rs",
        kind: Kind::FailPath,
        tool: "arm-none-eabi-g++ (variable)",
        reason: "a RELATIVE gate: the broken-precedence cross compile MUST fail with the \
                 div_t clash and the fixed one must succeed — only meaningful run together, \
                 and the raw cross-g++ object compile maps to no compile_check builder",
    },
    Entry {
        file: "platform_header_compile.rs",
        kind: Kind::FailPath,
        tool: "g++",
        reason: "the bare-metal heap-WITHOUT-malloc cell MUST fail to compile (#38 negative); \
                 the 9 POSITIVE cells already moved to cxx-syntax fixtures (phase-329 W5)",
    },
    // ---- RUNTIME BUILD EXCEPTIONS (positive, but un-prebuildable) ----
    Entry {
        file: "size_probe_verify.sh",
        kind: Kind::RuntimeException,
        tool: "cargo build",
        reason: "build-DETERMINISM soak: it repeatedly `cargo clean`+rebuilds to prove the \
                 generated header sizes don't flake — the repeated build IS the assertion, \
                 antithetical to a cached artifact. (It compared two NROS_SIZES_PROBE_MODE \
                 values until issue 0464 deleted the polling mode; now it also guards that \
                 neither fallback comes back.)",
    },
    // (borrowed_c_e2e.sh / borrowed_cpp_e2e.sh removed 2026-08-05 — they were
    //  orphaned + bit-rotted dead code, deleted rather than registered. See
    //  docs/issues/0423.)
    Entry {
        file: "integration_px4.rs",
        kind: Kind::RuntimeException,
        tool: "make --just-print",
        reason: "a DRY-RUN make (`--just-print -n`) that enumerates PX4 targets under the \
                 external-modules location — it compiles nothing, so there is no artifact to \
                 prebuild",
    },
];

fn tests_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests")
}

/// Non-comment lines of a source file (rs `//`, sh `#`).
fn code_lines(text: &str, comment: &str) -> Vec<String> {
    text.lines()
        .filter(|l| !l.trim_start().starts_with(comment))
        .map(|l| l.to_string())
        .collect()
}

/// Does this `.rs` file invoke a compiler/build tool at runtime?
fn rs_invokes_build(text: &str) -> bool {
    let lines = code_lines(text, "//");
    let joined = lines.join("\n");
    // Literal compilers/configurers/make — always a compile/configure/make.
    for tool in [
        r#"Command::new("cmake")"#,
        r#"Command::new("make")"#,
        r#"Command::new("cc")"#,
        r#"Command::new("gcc")"#,
        r#"Command::new("g++")"#,
    ] {
        if joined.contains(tool) {
            return true;
        }
    }
    // cargo, but only with a build/check SUBCOMMAND (not `cargo tree`/`metadata`).
    if joined.contains(r#"Command::new("cargo")"#)
        && (joined.contains(r#""build""#) || joined.contains(r#""check""#))
    {
        return true;
    }
    false
}

/// Does this `.sh` file invoke a compiler/build tool at runtime?
fn sh_invokes_build(text: &str) -> bool {
    for line in code_lines(text, "#") {
        for needle in [
            "cargo build",
            "cargo check",
            "cargo test",
            "g++ -",
            "gcc -",
            "cmake -",
            "make -C",
        ] {
            if line.contains(needle) {
                return true;
            }
        }
    }
    false
}

#[test]
fn enforce_registry() {
    let dir = tests_dir();
    let registered: BTreeSet<&str> = REGISTRY.iter().map(|e| e.file).collect();

    // (1) Every registered file must still exist (catch a rename/deletion leaving
    //     a stale row).
    let mut missing = Vec::new();
    for e in REGISTRY {
        if !dir.join(e.file).is_file() {
            missing.push(e.file);
        }
    }
    assert!(
        missing.is_empty(),
        "negative-diagnostic registry lists file(s) that no longer exist: {:?} — \
         update REGISTRY",
        missing
    );

    // (2) Scan tests/ — any file that invokes a compiler/build at runtime and is
    //     NOT registered is an unsanctioned compile-at-test (0196 rule).
    let mut unsanctioned = Vec::new();
    for entry in fs::read_dir(&dir).expect("read tests dir") {
        let path = entry.expect("dir entry").path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        // The registry file names all the tools in its reasons; never self-flag.
        if name == "negative_diagnostic_registry.rs" {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let invokes = match ext {
            "rs" => rs_invokes_build(&text),
            "sh" => sh_invokes_build(&text),
            _ => false,
        };
        if invokes && !registered.contains(name.as_str()) {
            unsanctioned.push(name);
        }
    }
    unsanctioned.sort();
    assert!(
        unsanctioned.is_empty(),
        "unsanctioned compile-at-test — {} file(s) invoke a compiler/build tool at RUNTIME \
         but are not in the negative-diagnostic registry (AGENTS.md E1 / issue 0196):\n  {}\n\n\
         Fix one of two ways:\n  \
         - MOVE the build to the fixture stage (a `[[compile_check_fixture]]` row or a \
         build-stage recipe) and consume the prebuilt artifact; OR\n  \
         - if it is a genuine FAIL-path diagnostic (a configure/compile/link that MUST fail), \
         add a row to REGISTRY in {} with the tool + why it cannot be prebuilt.",
        unsanctioned.len(),
        unsanctioned.join("\n  "),
        file!(),
    );
}

/// Sanity: the registry has both kinds and no duplicate file rows.
#[test]
fn registry_well_formed() {
    let mut seen = BTreeSet::new();
    for e in REGISTRY {
        assert!(seen.insert(e.file), "duplicate registry row for {}", e.file);
        assert!(!e.reason.is_empty(), "empty reason for {}", e.file);
        assert!(!e.tool.is_empty(), "empty tool for {}", e.file);
    }
    assert!(
        REGISTRY.iter().any(|e| e.kind == Kind::FailPath),
        "registry has no FAIL-path entries — did the categorization break?"
    );
    assert!(
        REGISTRY.iter().any(|e| e.kind == Kind::RuntimeException),
        "registry has no runtime-exception entries"
    );
}
