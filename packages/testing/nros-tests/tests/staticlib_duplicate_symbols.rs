//! Phase 241.D (RFC-0042 D3) — staticlib duplicate-symbol validator (slice 1).
//!
//! The C/CMake link path links the standalone RMW archive
//! (`libnros_rmw_zenoh_staticlib.a` / xrce) next to `libnros_c.a`. Both are
//! self-contained Rust `crate-type=["staticlib"]` archives, so each bundles its
//! own copy of the shared Rust dependency closure (nros-core, nros-serdes,
//! nros-rmw-cffi, log, core, alloc, …) + the `nros_rmw_cffi_*` C shim. The link
//! reconciles those copies with `-Wl,--allow-multiple-definition`
//! (`NanoRosLink.cmake`).
//!
//! That flag is a **blind** mask: it silences *every* duplicate, including a real
//! ODR violation (e.g. an application/message/transport symbol defined twice) —
//! exactly the determinism hole RFC-0042 D3 closes. This validator makes the
//! masked set **explicit + asserted**: the duplicate defined-global symbols
//! between the two archives MUST all originate from the known shared-dependency
//! closure (or the `nros_rmw_cffi_*` / compiler-rt C set). Any duplicate from an
//! application / message / transport-unique crate is a real multiply-defined bug
//! that `--allow-multiple-definition` would otherwise hide. This is the
//! precondition for safely removing the flag (D3 slice 2+): once we know the
//! masked set is *only* shared-dep bundling, deduping it (single shared rlib /
//! `-Bsymbolic` / one staticlib) becomes a contained change.
//!
//! Additive — it does NOT change linking. It consumes a prebuilt archive pair
//! (the threadx/freertos cpp staticlib-link fixtures produce them); skips when no
//! pair or the llvm tools are absent (CLAUDE.md: fail/skip on unmet
//! preconditions, never silent-pass).

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Command,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root from packages/testing/nros-tests")
        .to_path_buf()
}

/// Crates whose symbols legitimately appear in BOTH staticlibs because both
/// archives statically bundle them (the shared Rust dependency closure + the
/// CFFI vtable shim). A duplicate from any crate NOT in this set is a real ODR
/// violation the `--allow-multiple-definition` flag would otherwise hide.
const ALLOWED_SHARED_CRATES: &[&str] = &[
    "nros_core",
    "nros_serdes",
    "nros_rmw",
    "nros_rmw_cffi",
    "nros_platform_api",
    "nros_platform_cffi",
    "nros_platform_posix",
    "log",
    "core",
    "alloc",
    "std",
    "heapless",
    "hash32",
    "hashbrown",
    "byteorder",
    "compiler_builtins",
    "portable_atomic",
    "critical_section",
    // the rustc-internal alloc/panic shims demangle under `__rustc` / `rustc`
    "__rustc",
    "rustc",
];

/// A non-mangled (C / runtime) duplicate symbol that is legitimately shared, not
/// an application ODR violation:
///   * the CFFI vtable shim (`nros_rmw_cffi_*`) + its registry,
///   * compiler/runtime symbols — compiler-rt builtins, the Rust EH personality,
///     LLVM-promoted local constants — which all carry a reserved prefix
///     (`__`, `anon.`, `DW.ref.`) no nano-ros C ABI symbol uses.
/// A real application C symbol (e.g. a message helper) defined in BOTH archives
/// has none of these shapes, so it is still flagged.
fn c_symbol_allowed(sym: &str) -> bool {
    sym.starts_with("nros_rmw_cffi_")
        // the platform C ABI the linked port (nros-platform-cffi/-posix)
        // implements — both staticlibs bundle the port, so its symbols
        // (`nros_platform_log_write`, `_alloc`, `_clock_us`, …) appear in both.
        || sym.starts_with("nros_platform_")
        || sym == "REGISTRY"
        || sym == "rust_eh_personality"
        // compiler-rt / libgcc builtins, Rust runtime shims (`__rust_*`,
        // `__rg_*`), `__compilerrt_*`, soft-float/int helpers — all `__`-prefixed.
        || sym.starts_with("__")
        // LLVM-promoted anonymous local constants / merged globals.
        || sym.starts_with("anon.")
        // DWARF EH personality/type references.
        || sym.starts_with("DW.ref.")
}

fn tool(name: &str) -> Option<String> {
    if Command::new(name).arg("--version").output().is_ok() {
        Some(name.to_string())
    } else {
        None
    }
}

/// `llvm-nm --defined-only --extern-only <archive>` → set of raw (mangled)
/// global symbol names.
fn defined_global_symbols(nm: &str, archive: &Path) -> BTreeSet<String> {
    let out = Command::new(nm)
        .args(["--defined-only", "--extern-only"])
        .arg(archive)
        .output()
        .unwrap_or_else(|e| panic!("spawn {nm}: {e}"));
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split_whitespace().nth(2).map(str::to_string))
        .collect()
}

/// Extract every crate identifier embedded in a Rust v0-mangled symbol. A v0
/// crate-id is `C[s<base62>_]<len><name>` (`Cs<hash>_9nros_core`, `C4core`, …);
/// a generic instantiation carries the crate-ids of *every* crate it touches
/// (the type's crate, the trait's crate, the instantiating crate), so requiring
/// the whole set to be shared-dependency crates is the correct ODR check —
/// unlike "leading demangled token", which mis-reads `<&str as Display>::fmt` as
/// crate `&str`. Returns the distinct crate names; empty for non-v0 symbols.
fn v0_crate_ids(sym: &str) -> Vec<String> {
    let b = sym.as_bytes();
    let mut out = BTreeSet::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'C' {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        // optional disambiguator `s<base62>_`
        if j < b.len() && b[j] == b's' {
            let mut k = j + 1;
            while k < b.len() && b[k] != b'_' && b[k].is_ascii_alphanumeric() {
                k += 1;
            }
            if k < b.len() && b[k] == b'_' {
                j = k + 1;
            } else {
                i += 1;
                continue;
            }
        }
        // <len>
        let len_start = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j == len_start {
            i += 1;
            continue;
        }
        let Ok(len) = sym[len_start..j].parse::<usize>() else {
            i += 1;
            continue;
        };
        // optional `_` separator (name starting with digit/underscore)
        if j < b.len() && b[j] == b'_' {
            j += 1;
        }
        if j + len <= b.len() {
            let name = &sym[j..j + len];
            if !name.is_empty()
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && name
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            {
                out.insert(name.to_string());
                i = j + len;
                continue;
            }
        }
        i += 1;
    }
    out.into_iter().collect()
}

/// Find a prebuilt `(libnros_c.a, libnros_rmw_zenoh_staticlib.a)` pair.
///
/// Prefers the dedicated build-stage fixture
/// (`build/link-determinism/`, produced by
/// `scripts/build/link-determinism-fixture.sh` — host-cheap, always reproducible,
/// the hard-gate source). Falls back to any co-located pair under an example
/// `build-zenoh` tree (so the validator still runs against the real cross link
/// when a cpp fixture happens to be built).
fn find_archive_pair(root: &Path) -> Option<(PathBuf, PathBuf)> {
    let fx = root.join("build/link-determinism");
    let fx_c = fx.join("libnros_c.a");
    let fx_rmw = fx.join("libnros_rmw_zenoh_staticlib.a");
    if fx.join(".compile-ok").is_file() && fx_c.is_file() && fx_rmw.is_file() {
        return Some((fx_c, fx_rmw));
    }
    let out = Command::new("find")
        .arg(root.join("examples"))
        .args(["-name", "libnros_rmw_zenoh_staticlib.a"])
        .output()
        .ok()?;
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let rmw = PathBuf::from(line);
        // the co-located nros-c archive lives under the same build-zenoh root at
        // nano_ros/packages/core/nros-c/libnros_c.a
        let build_root = rmw
            .ancestors()
            .find(|a| a.file_name().is_some_and(|n| n == "build-zenoh"))?;
        let nros_c = build_root.join("nano_ros/packages/core/nros-c/libnros_c.a");
        if nros_c.is_file() && rmw.is_file() {
            return Some((nros_c, rmw));
        }
    }
    None
}

#[test]
fn staticlib_duplicate_symbols_are_only_shared_deps() {
    let root = repo_root();
    let Some(nm) = tool("llvm-nm") else {
        nros_tests::skip!("llvm-nm not on PATH — D3 duplicate-symbol validator needs it");
    };
    let Some((nros_c, rmw)) = find_archive_pair(&root) else {
        nros_tests::skip!(
            "no prebuilt (libnros_c.a, libnros_rmw_zenoh_staticlib.a) pair under examples/ — \
             build a threadx/freertos cpp staticlib fixture first (e.g. `just qemu-riscv64-threadx \
             build-fixture-extras`)"
        );
    };

    let a = defined_global_symbols(&nm, &nros_c);
    let b = defined_global_symbols(&nm, &rmw);
    let dups: Vec<String> = a.intersection(&b).cloned().collect();
    assert!(
        !dups.is_empty(),
        "expected the two self-contained staticlibs to share the bundled dependency \
         closure, but found ZERO duplicate defined globals between\n  {}\n  {}\n\
         (did the archives change shape? the validator's premise no longer holds)",
        nros_c.display(),
        rmw.display(),
    );

    // Categorise each duplicate by the crate(s) embedded in its mangling. A v0
    // symbol is unexpected if ANY embedded crate-id is outside the shared
    // closure; a non-mangled C symbol must be in the shim/builtin allowlist.
    let mut unexpected = Vec::new();
    for sym in &dups {
        if sym.starts_with("_R") {
            let crates = v0_crate_ids(sym);
            let bad: Vec<&String> = crates
                .iter()
                .filter(|c| !ALLOWED_SHARED_CRATES.contains(&c.as_str()))
                .collect();
            if !bad.is_empty() {
                unexpected.push(format!(
                    "{sym}  (crate(s) {bad:?} — NOT in the shared-dependency closure)"
                ));
            }
        } else if !c_symbol_allowed(sym) {
            unexpected.push(format!("{sym}  (C symbol, not in shared-shim allowlist)"));
        }
    }

    assert!(
        unexpected.is_empty(),
        "RFC-0042 D3: {} duplicate symbol(s) between libnros_c.a and the RMW staticlib \
         originate OUTSIDE the shared-dependency closure — a real ODR violation that \
         `--allow-multiple-definition` is silently masking:\n{}\n\n\
         (total duplicates checked: {}, all others are legitimate shared-dep bundling)",
        unexpected.len(),
        unexpected
            .iter()
            .take(40)
            .map(|u| format!("  {u}"))
            .collect::<Vec<_>>()
            .join("\n"),
        dups.len(),
    );

    eprintln!(
        "D3 validator: {} duplicate defined-globals between libnros_c.a and the RMW \
         staticlib, ALL from the shared-dependency closure (nros-core/serdes/rmw-cffi/\
         log/core/alloc/heapless/… + nros_rmw_cffi_* C shim). No unexpected ODR dup — \
         `--allow-multiple-definition` is masking only legitimate shared bundling.",
        dups.len(),
    );
}

/// Phase 241.D (RFC-0042 D3) slice 3 — the fix direction, proven on host.
///
/// `--allow-multiple-definition` is needed only because the current link pulls the
/// RMW backend in with a broad `--whole-archive` (to force its register / ctor
/// symbols), which drags in EVERY archive member — including the shared closure's
/// strong defs that then collide. The deterministic fix (D3 item 3) is to force
/// only the backend's register entry via `-u <symbol>` and lazy-link the rest:
/// the duplicated closure symbols are COMDAT/weak, so lazy archive-member
/// selection dedups them with no `--allow-multiple-definition`.
///
/// This asserts that on host the **2-archive (C-only)** pair links **with
/// `-u nros_rmw_zenoh_register`, WITHOUT `--allow-multiple-definition`**, AND
///   * the forced register entry is actually included (the `-u` did its job), and
///   * there is exactly ONE `REGISTRY` instance — the cffi backend registry stays
///     single (localizing/duplicating it would split registration → the #48
///     `NoBackend` hazard).
///
/// SCOPE: this is the C-only path (`libnros_c.a` + the RMW staticlib). The real
/// C++ link ALSO pulls `libnros_cpp.a`, a THIRD archive that bundles
/// `nros-rmw-cffi` too — and its C exports (`nros_rmw_cffi_set_custom_transport`,
/// …) are STRONG, so the 3-archive cpp link still collides under `-u` (verified:
/// converting the root `CMakeLists.txt` zenoh link to `-u` broke the native cpp
/// build). Dropping the flag therefore needs `nros-rmw-cffi` deduped to a single
/// archive first (the D3 architectural slice). This test guards the C-only lazy
/// path; it does NOT claim the flag is removable for the cpp link.
#[test]
fn host_pair_links_via_u_force_without_allow_multiple_definition() {
    let root = repo_root();
    let Some(cc) = tool("cc") else {
        nros_tests::skip!("cc not on PATH — D3 host link-proof needs a host C compiler");
    };
    let Some(nm) = tool("llvm-nm") else {
        nros_tests::skip!("llvm-nm not on PATH — D3 host link-proof needs it");
    };
    let Some((nros_c, rmw)) = find_archive_pair(&root) else {
        nros_tests::skip!(
            "no staticlib pair — run `scripts/build/link-determinism-fixture.sh` first"
        );
    };
    // Only the host (posix) pair is link-checkable here; the example cpp archives
    // are cross objects this host `cc` can't link.
    if !nros_c.starts_with(root.join("build/link-determinism")) {
        nros_tests::skip!(
            "link-proof needs the host fixture pair (build/link-determinism); the \
             discovered pair is a cross archive — run the fixture script"
        );
    }

    let tmp = tempfile::tempdir().unwrap();
    let main_c = tmp.path().join("bare.c");
    std::fs::write(&main_c, "int main(void){return 0;}\n").unwrap();
    let exe = tmp.path().join("lkproof");

    // `-u` forces the register entry; NO `--allow-multiple-definition`; lazy
    // archive selection dedups the COMDAT/weak shared closure.
    let out = Command::new(&cc)
        .arg(&main_c)
        .args(["-Wl,-u,nros_rmw_zenoh_register"])
        .arg(&nros_c)
        .arg(&rmw)
        .args(["-lpthread", "-ldl", "-lm"])
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap_or_else(|e| panic!("spawn {cc}: {e}"));
    assert!(
        out.status.success(),
        "host staticlib pair FAILED to link with `-u nros_rmw_zenoh_register` and \
         WITHOUT `--allow-multiple-definition` — a real strong-symbol collision the \
         flag was masking (D3 slice-4 blocker):\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    let syms = Command::new(&nm).arg(&exe).output().unwrap();
    let listing = String::from_utf8_lossy(&syms.stdout);
    assert!(
        listing
            .lines()
            .any(|l| l.ends_with(" T nros_rmw_zenoh_register")
                || l.ends_with(" t nros_rmw_zenoh_register")),
        "`-u nros_rmw_zenoh_register` did not pull the backend register entry into \
         the image — forcing the entry is the whole point of the `-u` replacement",
    );
    let registry_defs = listing
        .lines()
        .filter(|l| {
            l.ends_with(" T REGISTRY")
                || l.ends_with(" D REGISTRY")
                || l.ends_with(" B REGISTRY")
                || l.ends_with(" t REGISTRY")
                || l.ends_with(" d REGISTRY")
                || l.ends_with(" b REGISTRY")
        })
        .count();
    assert_eq!(
        registry_defs, 1,
        "expected exactly ONE cffi `REGISTRY` instance in the linked image (single \
         shared registry), found {registry_defs} — a split registry is the #48 \
         `NoBackend` hazard",
    );

    eprintln!(
        "D3 slice 3: the C-only host pair (libnros_c.a + RMW staticlib) links with \
         `-u nros_rmw_zenoh_register` and NO `--allow-multiple-definition` — register \
         included, single REGISTRY. NOTE: the 3-archive C++ link (+ libnros_cpp.a) \
         still collides on the STRONG `nros_rmw_cffi_*` C exports, so dropping the flag \
         needs nros-rmw-cffi deduped to one archive first (the D3 architectural slice).",
    );
}
