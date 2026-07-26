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
//! Fixture: `metadata_cpp` (examples/workspaces/cpp), built by
//! `compile-check-fixtures.sh` during `build-test-fixtures`. On posix that
//! workspace configures talker_pkg/listener_pkg (msg deps: `std_msgs`)
//! alongside cpp_add_*_pkg and cpp_fib_*_pkg (msg deps:
//! `example_interfaces`) — disjoint subsets, each package calling
//! `nros_find_interfaces` from its own package.xml, all linked into the
//! native entries.
//!
//! The tests read the PREBUILT fixture rather than running cmake (issue 0034
//! / AGENTS.md "No compilation inside tests").

use std::{path::PathBuf, process::Command};

/// Every per-package FFI archive in the fixture, excluding cargo's
/// `deps/`-mangled copies (which are the same objects under a hashed name and
/// would register as false duplicates).
fn ffi_archives() -> nros_tests::TestResult<Vec<PathBuf>> {
    // Anchor on a file the fixture always emits, so a missing fixture fails
    // with the standard prebuilt hint rather than an empty-vec pass.
    let metadata =
        nros_tests::fixtures::require_cmake_fixture("metadata_cpp", "nros-metadata.json")?;
    let root = metadata
        .parent()
        .expect("fixture metadata always has a parent dir")
        .to_path_buf();

    let mut out = Vec::new();
    let mut stack = vec![root.join("src")];
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
            let s = a.to_str()?;
            let after = s.split("/src/").nth(1)?;
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
