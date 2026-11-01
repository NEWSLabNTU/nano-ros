---
id: 2167
title: "Removing a materialized entry and rebuilding reports success but leaves the materialized program's binary in place"
status: resolved
type: bug
area: [cli, build]
severity: medium
found: 2026-10-31
related: [2175]
---

## What

Measured on `examples/workspaces/realtime-rust` (2026-10-31):

1. `nros build linux` builds the generated entry
   `build/posix-zenoh/linux_entry/` into
   `build/posix-zenoh/linux_entry/target/debug/linux_entry`.
2. `nros materialize linux`, edit `src/linux_entry/src/main.rs`, then
   `nros build linux`. Cargo runs with `--manifest-path src/linux_entry/Cargo.toml`
   and the same target dir, and writes the same `linux_entry` binary.
3. `rm -r src/linux_entry`, then `nros build linux`. That exits 0, but the
   binary keeps step 2's mtime and still runs the materialized program (it
   printed the edited `main.rs`'s lines). Only `touch`ing the regenerated
   `main.rs` made cargo relink.

The two packages share a name, a bin name and a target dir, and
`target/debug/deps` held one `linux_entry-<hash>` for both. So cargo judged the
generated unit fresh against the other package's output.

## Why it matters

A green `nros build` followed by running the printed artifact path runs a
program whose source no longer exists: a museum binary by construction.

## Direction

Give the materialized entry its own target dir (or its own `-C metadata` via a
distinct package identity), or have `nros build` invalidate the shared target
when an entry's ownership flips (the stamp in `builder/materialize.rs` already
records it). Acceptance: the three steps above, with the binary's output
changing back at step 3 and no manual `touch`.

## Resolution (2026-11-01)

The cargo road records which root last built into an image's target dir, in
`<target>/.nros-entry-root` (`builder::materialize::EntryRoot`). When this
build's manifest dir differs, `nros build` first runs
`cargo clean -p <entry>` with the same `--manifest-path`, `--config` (and
`build.build-dir`, `--target-dir`, `--offline`) as the build, then records the
new root. A target dir with content but no marker predates this and is cleaned
once. `--dry-run` prints the clean when one would run. The clean removes only
the entry package's units, never its dependencies'.

Verified on `examples/workspaces/realtime-rust`, using a 24-byte `#[used]`
marker string added to the materialized `main.rs` and checked with `strings`
on the printed artifact:

| step | build | marker in binary |
| --- | --- | --- |
| materialize + edit, `nros build linux` | rc 0, clean ran | 1 |
| `rm -r src/linux_entry`, `nros build linux` | rc 0, clean ran | **0** |
| `nros build linux` again (no flip) | rc 0, no clean | 0 |
| negative control: same as row 2 with the marker forged to the generated root | rc 0, no clean | **1** (the bug) |

Unit test: `builder::materialize::tests::an_entry_root_change_is_seen_until_it_is_recorded`.
