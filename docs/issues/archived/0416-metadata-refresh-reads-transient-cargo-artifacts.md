---
id: 416
title: "`nros sync`'s source digest walks isolated `target-*` dirs and races cargo's temporaries"
status: resolved
type: bug
area: build
related: [phase-337, phase-307, issue-0400]
resolved_in: "phase-337 W8.a"
---

## Symptom

`just build-test-fixtures lane=native` fails at a random Rust fixture with a
read error naming a file that plainly is not a source:

```
Error: read /…/examples/native/rust/talker/target-tls/nros-fast-release/deps/rmetaPn2VWn/full.rmeta
Caused by: No such file or directory (os error 2)
Location: nros-cli-core/src/orchestration/metadata_refresh.rs:392:14
```

The named path varies between runs — `rmetaXXXXXX/full.rmeta`,
`incremental/**/*.o`, `deps/rustcXXXXXX/symbols.o` — which is the tell.

Reproduced on a clean `main` (`60e9de7a0`) with a CLI built from that commit,
after `rm -rf examples/native/rust/talker/target-tls`, so it is not a
consequence of any in-flight branch.

## Root cause

`metadata_refresh::collect_sources` prunes by EXACT directory name:

```rust
const SKIPPED_DIRS: &[&str] = &["target", "build", "generated", "metadata", "node_modules"];
```

`examples/fixtures.toml` gives feature-variant rows their own build directory
via `target_dir =` — `target-tls`, `target-zenoh`, `target-xrce`,
`target-cyclonedds`, `target-zero-copy`. None of those equal `"target"`, so the
walker descends into a live cargo build directory and collects tens of
thousands of build artifacts as if they were package sources.

`source_digest` then reads every collected path to hash it. Cargo is
concurrently creating and deleting its own temporaries in that same tree, so
the read loses the race and the whole fixture build aborts.

Two things made it hard to place: the digest is a *freshness* mechanism, so the
failure appears during a BUILD and names a build artifact, and the isolated
target dirs only exist for the feature-variant rows — the plain rows use
`target/`, which the exact-name skip does catch.

## Fix (landed with phase-337 W8.a)

`collect_sources` now also skips anything `is_cargo_build_dir` recognises:

* a directory containing `CACHEDIR.TAG` — cargo writes one into every target
  directory it creates, and that spec exists so tools can identify build
  directories. Authoritative, and it catches target dirs with arbitrary names.
* a `target-` name prefix — the tag only appears once cargo has actually run,
  so a freshly-declared `target_dir` that has not been built yet is skipped
  rather than hashed.

Both, not either: the tag alone leaves a first-build window, and the prefix
alone re-creates the same exact-match brittleness in a new spelling.

Note what is NOT the fix: adding `"target-tls"`, `"target-zenoh"`, … to
`SKIPPED_DIRS`. That is a list which must be kept in step with
`examples/fixtures.toml`'s `target_dir` values by hand, and the next
`target_dir` anyone adds re-opens the bug — the same shape of defect as the
hand-written exclude lists issue 0287 replaced with a derived one.
