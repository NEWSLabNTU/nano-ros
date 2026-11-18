# Phase 580 — `include = [..]`: bringups share `system.toml` fragments

**Status (2026-11-18). OPEN: W1 in progress; W2 and W3 follow.** Implements
[RFC-0024](../design/0024-multi-node-workspace-layout.md) §5 "Shared
fragments" and closes
[issue 2587](../issues/2587-multi-bringup-shared-system-fragments.md). The
user ruled option (b) on 2026-11-18, as relayed by the nros 241 manager:

- an `include = ["<path>", …]` key, expanded by the one `system.toml` loader;
- paths are relative to the including file;
- cycles are refused;
- an include may not set `[system].name`.

Owner: nros 243.

## Why

Several example workspaces hold several bringups (`workspaces/cpp` 5,
`workspaces/c` 4, `workspaces/rust` 4), and each states its shared topology
and knob tables itself. A fragment stated once is only safe if every reader
sees it. A reader that reads the file raw would build from a different
system than the others, with nothing reported. So the work is the readers,
one road at a time, and the key is refused on any road that cannot expand
it yet.

Every PR must work on its own: until a road expands includes, that road
refuses a file with an `include` and never reads it raw. The user-facing
docs ship with the PR that makes every road honour the key.

## W1 — the Rust expander, every Rust reader, the gate

- **Status:** OPEN (in progress on branch `issue-2587-system-include`).
- `packages/tooling/nros-system-include`: `read` (expanded table plus every
  file read), `read_to_string` (the same with `std::fs::read_to_string`'s
  signature, `NotFound` kept), `read_own_text` (for an editor), and
  `has_include`. It holds the rules of RFC-0024 §5, with a test for each.
- Every non-test Rust reader of a `system.toml` reads through it: the CLI,
  `cargo-nano-ros`, `nros-orchestration-ir` and `nros-macros`.
  - The issue estimated 49 readers from a literal grep. The census found
    more, reading through a bound variable.
  - The gate found five more after that: `cargo_node`, `locate`,
    `rmw_config` ×3 and `router_rule`.
  - The editors (`new_entry`, `new_node`, `new_workspace`,
    `scaffold_deploy`) read their own text.
  - `nros new --workspace --from` moves the file byte for byte, so it reads
    the file's own text, and it refuses a file with an `include`, whose
    relative paths a move would break.
- `nros-build-paths::project_entry` keeps reading the file's own text, on
  purpose. `[sources]` / `[tools]` (this machine's paths) may not be set by
  a fragment, which the expander refuses, so nothing is lost. The crate
  stays free of a TOML parser: giving it one put `toml`, `serde` and a
  second `syn` into the build-script graph of every embedded board root
  (measured: 19 locked roots' locks moved).
- The roads that cannot expand yet refuse a file with an `include`:
  - `nano_ros_workspace`'s image-row check (`cmake/NanoRosSystemInclude.cmake`);
  - the PX4 module's RMW read;
  - the colcon task's image probe.
- Gate `check-system-toml-one-reader`:
  - refuses a non-test Rust `fs::read_to_string` of the file outside the
    crate, either named directly or through a variable bound within 25
    lines;
  - refuses a tracked `system.toml` using `include`, while the Python check
    scripts read raw;
  - its selftest runs on the normal path, and a mutation of a real site
    turns it red.
- **Acceptance:** the crate's tests; the gate green; the CLI builds.

## W2 — Python

- **Status:** OPEN.
- One Python reader (`scripts/lib/`) with the same rules, which every check
  script that reads a `system.toml` uses. Its results are compared with the
  Rust expander on shared cases, so the two cannot drift. The colcon task
  reads through it.
- The gate's tracked-`include` half goes.
- **Acceptance:** a cross-check of the two expanders over the same cases;
  the gate refuses a direct Python read.

## W3 — CMake, and the user-facing docs

- **Status:** OPEN.
- The CMake text reads take the expanded system from `nros` (as the
  configure's other facts do), and the refusals of W1 go.
- An example workspace shares a fragment between two bringups, and both
  images build from it (the issue's "done when").
- The book's `system.toml` reference and a guide section document the key.
- **Acceptance:** that workspace builds on the cargo and cmake roads; issue
  2587 resolves.
