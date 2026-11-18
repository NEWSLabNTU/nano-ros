# Phase 580 — `include = [..]`: bringups share `system.toml` fragments

**Status (2026-11-19). COMPLETE: W1 (#4271), W2 (#4279) and W3 landed, each as its own PR.** Implements
[RFC-0024](../../design/0024-multi-node-workspace-layout.md) §5 "Shared
fragments" and closes
[issue 2587](../../issues/archived/2587-multi-bringup-shared-system-fragments.md). The
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

- **Status:** DONE (#4271).
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

- **Status:** DONE (#4279).
- `scripts/lib/system_toml.py` is the one Python reader, with the same rules
  and messages as the Rust one. It offers `load` (the expanded table),
  `load_with_files`, `text` (the expansion as TOML text, for a reader that
  scans text) and `has_include`.
- The two readers cannot drift. Both run the shared cases in
  `packages/tooling/nros-system-include/cases/` (13: each rule, each
  refusal). The Rust test `every_shared_case_expands_as_expected` and the
  Python selftest assert the same `expect.toml`. `text()` reads back as the
  expansion for every case and for all 179 tracked `system.toml`.
- Every Python reader of a system's meaning reads through it:
  - `fixtures-manifest`;
  - 15 check scripts;
  - `docs/gen-example-readmes`;
  - `lib/{image_locator,template_projects,priority_plan,leaf_target}`.
  - The regex readers take `text()`, so a one-call change makes them see
    every fragment.
  - `leaf_target` keeps its zero-dependency common case: it imports the
    reader only when the file names an include.
- A deliberate own-text read says why on its line
  (`# system.toml own text: <reason>`). There are three: the colcon task's
  include refusal, `leaf_target`'s include probe, and a selftest reading
  back its own file. Writes are not reads.
- The colcon task keeps its W1 refusal. An installed colcon plugin cannot
  import `scripts/lib`, so it moves to W3 with the CMake road, both reading
  the expansion from `nros`.
- `check-system-toml-one-reader` gains the Python half:
  - it refuses a `read_text` / `open` / `tomllib.load(s)` of the file
    outside the reader, named directly or through a variable bound in the
    same function;
  - it runs the Python reader's case selftest;
  - its tracked-`include` half is gone, since every Python reader now
    expands.
  - Mutation-checked: one migrated line put back is red.
- **Acceptance:** the shared cases agree in both languages; the gate
  refuses a direct Python read.

## W3 — CMake, colcon, freshness, and the user-facing docs

- **Status:** DONE.
- `nros dev image system-toml <bringup> [--out F] [--files-out F]` prints a
  bringup's `system.toml` expanded, and every file it read. It writes only
  when the text changes.
- CMake: `cmake/NanoRosSystemInclude.cmake`.
  - `nros_system_toml_expanded()` hands a line reader the file itself when
    it names no include (no CLI call), and otherwise the expansion, written
    into the build dir, with every fragment a configure dependency.
    `nano_ros_workspace`'s image-row check and the PX4 module's RMW read use
    it, and W1's refusals are gone.
  - `nros_system_toml_watch()` is the one spelling of "this configure reads
    that system.toml". It replaces the four bare `CMAKE_CONFIGURE_DEPENDS`
    lines on the file (leaf entity facts, both package-xml readers, the
    Zephyr image kconfig).
- The colcon task reads the expansion `nros` prints, instead of refusing.
- Freshness: every watcher watches the fragments too.
  - `nros::main!` tracks them as `include_bytes!` inputs
    (`track_system_toml_fragments`, tested).
  - `nros-build` emits a `cargo:rerun-if-changed` per fragment.
  - The CMake watchers use the helper above.
  - `nros configure`'s input digest (`configure_stamp::input_files`) found
    inputs by file name, and a fragment is not named `system.toml`. So a
    fragment edit left the stamp current and the configure skipped, on every
    road. Each `system.toml` input now adds its fragments (tested). The CMake
    configure hook registers the stamp's inputs, which makes the fragment a
    configure dependency there too. Measured: a fragment-only edit prints
    `inputs changed` and re-runs the configure; the cargo image then carries
    the new value.
  - Without this, a fragment edit would leave an image reading as fresh with
    the old value: issue 0196's class.
- Gate `check system-include` (`tests/cmake-system-include-tests.sh`):
  - P: no include, the file itself;
  - E: the expansion, with the fragment a configure input;
  - R: a key in two files is refused;
  - W: `nano_ros_workspace` sees per-image rows that sit in a fragment.
  - A mutant that stops expanding fails E, R and W.
- Examples: `examples/workspaces/{rust,c}` share `shared/system-defaults.toml`
  (`[system] rmw`, `domain_id`, `[image_defaults] rmw`) between their nine
  bringups. Each expansion equals that bringup's content before the change,
  so it is a semantic no-op, verified file by file. `default_launch` stays
  per bringup: `levels_bringup` differs.
- Book: `reference/system-toml.md` "Sharing a fragment between bringups".
- **Acceptance:** a `linux` image from two bringups that share the fragment
  builds on the cargo road (`workspaces/rust`) and the cmake road
  (`workspaces/c`); issue 2587 resolves.
