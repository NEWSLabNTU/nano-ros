---
id: 699
title: "`nros sync` fails `Metadata(NameTooLong)` when the workspace sits in a deep path — four frames, none naming the length limit"
status: resolved
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

## Resolution

Two defects, one visible and one that hid it.

**The cause — not `sun_path`.** `SourceLocation::caller()` in
`packages/api/nros/src/node_metadata.rs` recorded

```rust
artifact: copy_str(location.file())?,
```

into a `MetadataString`, which is `heapless::String<METADATA_STRING_CAPACITY>`
with `METADATA_STRING_CAPACITY = 128`. rustc emits `Location::file()` ABSOLUTE
whenever the recording crate is compiled as a path dependency from another
directory — which is exactly how the metadata harness builds it — so the length
of that field is set by where the user keeps their workspace, and nothing else.
Past 128 bytes, `copy_str` returns `NameTooLong`. No socket was involved; the
issue's `sun_path` guess was wrong.

Front-truncation is lossless here, which is what makes it the fix rather than a
dodge: `relativise_source_artifacts` (same file's CLI counterpart,
`metadata_build.rs`) rewrites that path to be package-relative immediately
afterwards, so the prefix is discarded either way. `copy_str_keep_tail` keeps
the tail, cuts on a `/` boundary, and marks the cut with a leading `…/`; a value
that fits is returned untouched.

**The reason nobody could see it.** `first_diagnostic` looked for a line
starting with `error`, and a harness that COMPILES and then PANICS has none — so
it fell through to "last non-empty line", which is `note: run with
RUST_BACKTRACE=1 …`. That is why `metadata_build.rs:494` reported an exit code
and a component and nothing else, and why the only way to read the real cause
was to re-run the staged harness by hand. `panic_message` now takes the line
after the `panicked at …` header — keyed on POSITION, not on a substring search
over the stream, for the same reason `nros_tests::skip_marker` is (a build that
merely prints the word must not be misreported).

### Evidence

Same tree, same host, same CLI; only the workspace prefix differs. Metadata
cache cleared between runs.

| tree | result |
| --- | --- |
| pristine, 127-char workspace path | `EXIT=1` — `metadata-mode harness failed (exit -1) for component 'listener'` |
| with the fix | `EXIT=0` — `sync: source metadata — 2 rebuilt, 0 already current` |

The recorded artifact shows the truncation doing its job:

```json
"artifact": "…/deeply-nested-workspace-dir/.../ws/src/listener_pkg/src/lib.rs"
```

Tests: `a_path_longer_than_the_buffer_keeps_its_tail` (nros),
`a_harness_panic_is_the_diagnostic_not_the_backtrace_note` and
`a_rustc_error_still_outranks_a_panic` (nros-cli-core).

## Likely cause (as filed — the `sun_path` guess was wrong)

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
