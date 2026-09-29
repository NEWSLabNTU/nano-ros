//! Rebuild edges for the files a `cc::Build` actually READ (issue 1570).
//!
//! # The class
//!
//! cc-rs compiles whatever it is handed and tells cargo nothing about it. A
//! build script that compiles sources it does not own must therefore declare
//! them itself, and it can only declare what it can name: a source it was
//! handed, perhaps — never the headers those sources include, because only
//! the compiler knows that set.
//!
//! The NuttX image is linked by exactly such a build script
//! (`nros-board-common::nuttx_ffi_build`), which compiles every component TU of
//! the image from an env LIST. It watched the list, not the files, and no
//! header at all — so an edit to a component `.c`, to `component.h`, or to the
//! committed NuttX config snapshot left cargo's fingerprint unchanged and the
//! image a museum binary (measured: `__nros_c_inst_ctrl_pkg` stayed `0x238` in
//! the image while the object cmake rebuilt beside it read `0x288`). CLAUDE.md's
//! issue-0475 class, one lane over: a file-level input with no rebuild edge.
//!
//! # The mechanism
//!
//! Ask the compiler. `-MMD` makes gcc/clang write a Makefile depfile beside
//! each object (`<obj>.o` → `<obj>.d`) naming the source and every
//! non-system header it opened. After the compile, every name in those files
//! becomes a `cargo:rerun-if-changed`. Cargo also copies build-script
//! `rerun-if-changed` paths into the dep-info file it writes for the final
//! artifact, so a cmake/ninja rule that consumes that dep-info as its `DEPFILE`
//! (the NuttX lane does, issue 0820) inherits the same edges — one mechanism
//! repairs both layers.
//!
//! The set is recorded on the run that compiled, which is exactly the set the
//! current objects depend on. A newly added `#include` is picked up because
//! adding it edits a file already in the set.
//!
//! # Why a depfile is CONSUMED
//!
//! cc-rs recompiles every file on every build-script run, so each run writes a
//! fresh `.d` for every object it still builds. [`emit_header_deps`] deletes
//! each depfile once it has declared its contents. A source later dropped from
//! the list therefore cannot leave an old `.d` behind to be replayed — which
//! would declare a path that may no longer exist, and cargo treats a missing
//! `rerun-if-changed` path as permanently dirty: the treadmill. It also scopes
//! the replay to the compile just finished, so several `cc::Build`s sharing one
//! `OUT_DIR` can each call it after their own `compile()` without reading or
//! losing one another's.
//!
//! `-MMD`, not `-MD`: system headers (`-isystem`, the toolchain's own) are
//! omitted. They move only with a toolchain change, which is already a
//! fingerprint change of its own, and listing a few hundred libc headers per
//! TU would buy nothing.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

/// Make every compile of `build` write a Makefile depfile beside its object.
pub fn track_header_deps(build: &mut cc::Build) -> &mut cc::Build {
    build.flag("-MMD")
}

/// Every `<x>.d` under `dir` that sits beside a `<x>.o` — i.e. a compiler
/// depfile, never an unrelated file that happens to end in `.d`.
fn depfiles(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|e| e == "d") && p.with_extension("o").is_file() {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Parse one Makefile-format depfile into its prerequisites.
///
/// Handles what gcc/clang emit: `target: a b \` continuations, `\ ` escaped
/// spaces, `$$` for `$`, and the phony `hdr.h:` stanzas `-MP` would add.
pub fn parse_depfile(text: &str) -> Vec<String> {
    // Join continuations first; a `\` before a newline is a line splice.
    let joined = text.replace("\\\r\n", " ").replace("\\\n", " ");
    let mut deps = Vec::new();
    for line in joined.lines() {
        // Split at the first rule colon: a ':' followed by whitespace or EOL
        // (a Windows drive letter `C:\x` is followed by neither).
        let bytes = line.as_bytes();
        let mut colon = None;
        for (i, &b) in bytes.iter().enumerate() {
            if b == b':' && (i + 1 == bytes.len() || bytes[i + 1].is_ascii_whitespace()) {
                colon = Some(i);
                break;
            }
        }
        let Some(colon) = colon else { continue };
        let rest = &line[colon + 1..];
        let mut cur = String::new();
        let mut chars = rest.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\\' if chars.peek() == Some(&' ') => {
                    cur.push(' ');
                    chars.next();
                }
                '$' if chars.peek() == Some(&'$') => {
                    cur.push('$');
                    chars.next();
                }
                c if c.is_whitespace() => {
                    if !cur.is_empty() {
                        deps.push(std::mem::take(&mut cur));
                    }
                }
                c => cur.push(c),
            }
        }
        if !cur.is_empty() {
            deps.push(cur);
        }
    }
    deps
}

/// Read, then delete, every compiler depfile under `out_dir`; return the files
/// they name, deduplicated and sorted.
pub fn take_header_deps(out_dir: &Path) -> BTreeSet<String> {
    let mut all = BTreeSet::new();
    for d in depfiles(out_dir) {
        if let Ok(text) = fs::read_to_string(&d) {
            all.extend(parse_depfile(&text));
        }
        let _ = fs::remove_file(&d);
    }
    all
}

/// Emit `cargo:rerun-if-changed` for every file the compile just finished
/// read. Call AFTER each `compile()` whose `cc::Build` went through
/// [`track_header_deps`]. Returns how many paths were declared.
///
/// Panics if the compile left no depfile at all: that means `-MMD` never
/// reached the compiler, and declaring nothing is the defect this exists to
/// close, so it must not pass silently.
pub fn emit_header_deps(out_dir: &Path) -> usize {
    let deps = take_header_deps(out_dir);
    assert!(
        !deps.is_empty(),
        "issue 1570: no compiler depfile under {} after a compile — was \
         `track_header_deps` applied to this cc::Build?",
        out_dir.display()
    );
    for p in &deps {
        println!("cargo:rerun-if-changed={p}");
    }
    deps.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gcc_continuations() {
        let d = "/o/x.o: /src/a.c /inc/b.h \\\n /inc/c.h \\\n  /inc/d.h\n";
        assert_eq!(
            parse_depfile(d),
            vec!["/src/a.c", "/inc/b.h", "/inc/c.h", "/inc/d.h"]
        );
    }

    #[test]
    fn parses_escaped_spaces_and_dollars() {
        let d = "x.o: /my\\ dir/a.c /p$$q/b.h\n";
        assert_eq!(parse_depfile(d), vec!["/my dir/a.c", "/p$q/b.h"]);
    }

    #[test]
    fn phony_stanzas_contribute_nothing_new() {
        let d = "x.o: a.c b.h\n\nb.h:\n";
        assert_eq!(parse_depfile(d), vec!["a.c", "b.h"]);
    }

    #[test]
    fn only_depfiles_beside_objects_are_read_and_consumed() {
        let dir = std::env::temp_dir().join(format!("nros-cc-hdr-{}", std::process::id()));
        let sub = dir.join("h");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("a.o"), b"").unwrap();
        fs::write(sub.join("a.d"), "a.o: /s/a.c /i/a.h\n").unwrap();
        // A `.d` with no object beside it is not ours — never read, never removed.
        fs::write(dir.join("other.d"), "z: /not/mine\n").unwrap();
        let got = take_header_deps(&dir);
        assert_eq!(
            got.into_iter().collect::<Vec<_>>(),
            vec!["/i/a.h", "/s/a.c"]
        );
        assert!(
            !sub.join("a.d").exists(),
            "a replayed depfile must be consumed"
        );
        assert!(dir.join("other.d").exists());
        // A second take (the next compile in the same OUT_DIR) sees nothing old.
        assert!(take_header_deps(&dir).is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }
}
