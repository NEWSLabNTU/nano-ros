---
id: 699
title: "`nros sync` fails `Metadata(NameTooLong)` when the workspace sits in a deep path — four frames, none naming the length limit"
status: open
type: bug
severity: medium
area: cli, orchestration
related: [phase-368]
---

## Symptom

The canonical copy-out template (`examples/templates/multi-node-workspace`),
copied to a ~100-char-deep directory, fails `nros sync` at the metadata stage:

```
sync: wrote [patch.crates-io] → <ws>/.cargo/config.toml
sync: metadata listener_pkg::listener — sources changed, rebuilding
Error: refresh source metadata for `listener_pkg`
Caused by:
    metadata-mode harness failed (exit -1) for component 'listener': …
Location:
    nros-cli-core/src/orchestration/metadata_build.rs:494:21
```

Running the staged harness by hand names the real error:

```
$ cd <ws>/build/nros-metadata/metadata-probe/listener && cargo run
thread 'main' panicked at src/main.rs:7:10:
component register (metadata mode): Metadata(NameTooLong)
```

**The identical tree in a 34-char path syncs clean** (`nros sync` → metadata
2 rebuilt → `cargo build` green). Reproduced 2026-08-20 with the same files,
same host, same CLI — only the prefix differs.

## Likely cause

`NameTooLong` from a register call whose failure depends only on the CWD depth
points at a fixed-size path buffer — the classic instance is an `AF_UNIX`
socket path (`sun_path`, 108 bytes) bound somewhere under the workspace build
tree; a `heapless`-style bounded string holding an absolute path would present
the same way. Not confirmed; the harness's `src/main.rs` and the metadata-mode
register path are where to look.

## Why it matters more than a corner case

The failure lands on exactly the surface phase-368 is promoting: "copy the
template anywhere and run two commands". A user's home-dir nesting is not ours
to bound, and the error's four frames name a component and an exit code but
never a length, a path, or a limit — undiagnosable without re-running the
staged harness by hand, which nothing tells the user exists.

## Fix sketch

Find the bounded buffer; either lift it (socket in `$XDG_RUNTIME_DIR`/`/tmp`
with a short hashed name, the standard dodge for sun_path) or make the error
name the offending path and the limit. Either way the harness's panic should
travel up — `metadata_build.rs:494` currently reports only "exit -1".
