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

/// Non-mangled C symbols both archives legitimately export (the CFFI vtable shim
/// + its registry) and compiler-rt builtins.
fn c_symbol_allowed(sym: &str) -> bool {
    sym.starts_with("nros_rmw_cffi_")
        || sym == "REGISTRY"
        // compiler-rt / libgcc soft-float + int builtins both archives may carry
        || (sym.starts_with("__") && {
            let s = &sym[2..];
            s.ends_with("di3")
                || s.ends_with("si3")
                || s.ends_with("ti3")
                || s.ends_with("df3")
                || s.ends_with("sf3")
                || s.starts_with("udiv")
                || s.starts_with("umod")
                || s.starts_with("div")
                || s.starts_with("mod")
                || s.starts_with("mul")
                || s.starts_with("ashl")
                || s.starts_with("lshr")
                || s.starts_with("ashr")
                || s.starts_with("clz")
                || s.starts_with("ctz")
                || s.starts_with("ffs")
                || s.starts_with("popcount")
                || s.starts_with("bswap")
                || s.starts_with("fix")
                || s.starts_with("float")
                || s.starts_with("extend")
                || s.starts_with("trunc")
                || s.starts_with("cmp")
                || s.starts_with("unord")
                || s.starts_with("add")
                || s.starts_with("sub")
        })
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

/// Find a prebuilt `(libnros_c.a, libnros_rmw_zenoh_staticlib.a)` pair under the
/// example build trees. Returns the first co-located pair (same `build-zenoh`).
fn find_archive_pair(root: &Path) -> Option<(PathBuf, PathBuf)> {
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
