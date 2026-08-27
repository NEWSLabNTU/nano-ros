//! Mixed msg-dep subsets across one workspace (issue 0277).
//!
//! When two packages in the same workspace resolve DIFFERENT interface
//! subsets, the generated FFI archives all land on one link line. The
//! retired topo-last superset design computed its closure per
//! `nros_find_interfaces` CALL, so those archives overlapped: the same
//! `nros_cpp_*` symbols were defined in more than one of them (duplicate
//! definitions at link) or, in the other ordering, a symbol the smaller call
//! never generated went missing. Workspaces worked around it with a
//! union-closure shim package forced first in SUBDIRS.
//!
//! phase-306 W1 replaced that with a per-package FFI crate: the split
//! types/exports closure means a crate carries dependency TYPES but exports
//! only its OWN symbols, so any combination of archives links. These tests
//! gate that property — it is the reason the shim package is unnecessary and
//! the reason issue 0277 is closed.
//!
//! Fixture: the `workspace-cpp-native` row (`examples/workspaces/cpp`), built
//! by `workspace-fixtures-build.sh` during `build-test-fixtures`. On posix that
//! workspace configures talker_pkg/listener_pkg (msg deps: `std_msgs`)
//! alongside the action and service packages (msg deps: `action_msgs`,
//! `example_interfaces`, `builtin_interfaces`) — disjoint subsets, each package
//! calling `nros_find_interfaces` from its own package.xml, all linked into the
//! native entries.
//!
//! It read the `metadata_cpp` COMPILE-CHECK fixture until phase-383 W10.a. That
//! fixture was retargeted onto `examples/templates/multi-node-workspace-cpp`
//! when `examples/workspaces/cpp` lost its hand-written root, and the template
//! cannot carry this property: both its packages depend on `std_msgs` alone, so
//! there is exactly ONE subset and the disjointness this file exists to prove
//! has nothing to stand on. The workspace with the disjoint subsets is the one
//! to read, and the workspace-fixture row is how to reach it.
//!
//! The tests read the PREBUILT fixture rather than running cmake (issue 0034
//! / AGENTS.md "No compilation inside tests").

use std::{path::PathBuf, process::Command};

/// Every per-package FFI archive in the fixture, excluding cargo's
/// `deps/`-mangled copies (which are the same objects under a hashed name and
/// would register as false duplicates).
fn ffi_archives() -> nros_tests::TestResult<Vec<PathBuf>> {
    // Anchor on the entry binary, so a missing or unbuilt fixture fails with
    // the standard prebuilt hint rather than an empty-vec pass.
    nros_tests::fixtures::build_native_workspace_cpp_entry()?;
    let root = nros_tests::fixtures::groups::workspace_artifact_dir("workspace-cpp-native")?;

    // The whole build dir, not `src/` — a MIGRATED workspace's generated root
    // lives under `build/<coord>/` while its packages stay in the source tree,
    // so `nano_ros_workspace` gives each out-of-tree subdir a binary dir under
    // `pkg/<name>/` (phase-383 W4). Naming one of the two layouts here would
    // make the walk silently empty on the other, which reads as "no archives
    // were generated" rather than "looked in the wrong place".
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip cargo's deps/ — hashed duplicates of the same archive.
                if path.file_name().is_some_and(|n| n == "deps") {
                    continue;
                }
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("libnano_ros_cpp_ffi_") && n.ends_with(".a"))
            {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Globally-defined `nros_cpp_*` symbols in one archive.
fn exported_symbols(archive: &PathBuf) -> nros_tests::TestResult<Vec<String>> {
    let out = Command::new("nm")
        .args(["-g", "--defined-only"])
        .arg(archive)
        .output();
    let out = match out {
        Ok(o) if o.status.success() => o,
        Ok(o) => nros_tests::skip!(
            "nm failed on {}: {}",
            archive.display(),
            String::from_utf8_lossy(&o.stderr)
        ),
        Err(e) => nros_tests::skip!("nm unavailable ({e}) — cannot inspect FFI archives"),
    };
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            // "<addr> T <symbol>" — only defined text symbols.
            let mut parts = line.split_whitespace();
            let sym = parts.next_back()?;
            let kind = parts.next_back()?;
            (kind == "T" && sym.starts_with("nros_cpp_")).then(|| sym.to_string())
        })
        .collect())
}

/// The workspace really does resolve more than one distinct subset — otherwise
/// the duplicate check below would pass vacuously on a single archive.
#[test]
fn mixed_subset_workspace_builds_per_package_ffi_crates() -> nros_tests::TestResult<()> {
    let archives = ffi_archives()?;
    assert!(
        archives.len() >= 2,
        "expected several per-package FFI archives in the mixed-subset workspace, found {}: {:#?}",
        archives.len(),
        archives
    );

    // Archives must live under more than one package dir — that is what makes
    // this a MIXED-subset workspace rather than one package's closure.
    let owning_pkgs: std::collections::BTreeSet<_> = archives
        .iter()
        .filter_map(|a| {
            // Both layouts: `src/<pkg>/` for an in-tree subdir, `pkg/<pkg>/`
            // for the out-of-tree binary dir a generated root produces.
            let s = a.to_str()?;
            let after = s
                .split("/pkg/")
                .nth(1)
                .or_else(|| s.split("/src/").nth(1))?;
            after.split('/').next().map(str::to_string)
        })
        .collect();
    assert!(
        owning_pkgs.len() >= 2,
        "expected FFI archives owned by >= 2 packages (disjoint msg-dep subsets); got {owning_pkgs:?}"
    );
    Ok(())
}

/// The property issue 0277 is about: no `nros_cpp_*` symbol is DEFINED by more
/// than one archive, so every combination of subsets links.
#[test]
fn interface_archives_carry_no_duplicate_exports() -> nros_tests::TestResult<()> {
    let archives = ffi_archives()?;
    let mut owner: std::collections::BTreeMap<String, PathBuf> = std::collections::BTreeMap::new();
    let mut duplicates: Vec<String> = Vec::new();

    for archive in &archives {
        for sym in exported_symbols(archive)? {
            if let Some(previous) = owner.get(&sym) {
                duplicates.push(format!(
                    "{sym}: defined in {} AND {}",
                    previous.display(),
                    archive.display()
                ));
            } else {
                owner.insert(sym, archive.clone());
            }
        }
    }

    assert!(
        duplicates.is_empty(),
        "issue 0277 regression — the per-package FFI split is leaking dependency \
         EXPORTS, so two archives define the same symbol and a mixed-subset \
         workspace will fail to link:\n{}",
        duplicates.join("\n")
    );
    assert!(
        !owner.is_empty(),
        "no nros_cpp_* exports found across {} archives — the symbol scan is \
         not measuring anything",
        archives.len()
    );
    Ok(())
}
