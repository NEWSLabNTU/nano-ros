---
id: 1267
title: "`nros setup` installs its packages one at a time — the loop is
  sequential in the CLI, so no caller can parallelise it"
status: open
type: tech-debt
area: cli, build
severity: medium
found: 2026-09-10
related: [issue-1266, issue-0374, issue-0500, rfc-0014]
---

## What this is

`cmd/setup.rs:342`:

```rust
for name in &packages {
    …
    let action = plan_install(tool, &host, &prefix);
    …
    let provenance = execute(&other, name, &tool.version, &prefix, &tool.front)?;
    lock.record(name, &provenance);
}
```

One package at a time: resolve, download, unpack, record, next. There is no
`rayon`, no `par_iter` and no spawn anywhere in the setup path — a grep across
`packages/cli` (excluding `third-party/`) finds concurrency only in
`test_support.rs`, `cargo-nano-ros/src/workflow.rs`, and play_launch.

This is not something a caller can work around. `just setup <plat>` is a thin
caller over `nros setup` — 81 call sites in `just/*.just`, zero `curl`/`wget`/
`pip`/`apt-get install` in any setup recipe, and `check-one-producer-per-tool`
holds it that way. So the serialisation is in the one place that does the work,
and both the user front door (`nros setup`) and the dev front door (`just setup
…`) inherit it.

## Why the packages are safe to parallelise

Each `[tool.*]` installs into `tool_prefix(root, name, version)` — a path keyed
by name AND version — so two packages never write the same directory. The
prefixes are disjoint by construction, not by convention.

## Why it is still not a two-line change

Four things are shared, and each needs a decision rather than a guess:

1. **The lock file.** `lock.record(name, &provenance)` mutates one `lock` and
   `lock.save(&lock_path)` writes once at the end. A join-then-record keeps the
   current single-writer shape; recording from inside the workers does not.

2. **`front_newest`.** `execute` fronts the tool into the shared store root
   after installing. Two concurrent fronts touch the same front directory, and
   the ordering rule they implement is issue 0500's: prefixes resolve
   newest-version-first, and a stale entry that shadows a fresh one is a bug
   that prints success on both paths. Fronting is the step that must stay
   ordered even if the installs do not.

3. **`bin_dirs`.** Pushed in loop order and folded onto the emitted
   CMakePreset's `environment.PATH`. PATH order is significant; a parallel loop
   must preserve the plan's order rather than completion order.

4. **Output.** `eprintln!("  {:<22} {}", name, …)` interleaved across workers is
   unreadable. Buffer per package and flush on completion, in plan order.

Two further limits worth naming so nobody is surprised by the speedup being
smaller than the package count. Source builds (`play_launch_parser`, `espflash`,
qemu) each already use the whole machine through cargo/ninja, so running two
concurrently mostly moves contention around — the win there is overlapping a
source BUILD with another package's DOWNLOAD, not two builds. And a concurrency
of N against one host saturates the link long before it saturates the CPU: on
the run this was found in, the measured rate was ~0.8 MB/s.

## Fix

A bounded worker pool over the already-resolved plan, defaulting to a small
number (4) and overridable. Keep the three ordered steps ordered: record, front,
and print in plan order after the join.

The plan/execute split this needs already exists — issue 0374 added a pre-pass
that resolves the whole plan before the first fetch, so `source_build_names`
could announce the long builds up front, and `plan_install` is documented as
pure.

Issue 1266 is the cheaper, independent half: overlapping the fetch of one
package with the unpack of the previous one needs none of the four decisions
above, because only one package is ever being installed.

## Where this was measured

A contained self-hosted runner bootstrap on this workstation, 2026-09-10: 24
`nros setup` invocations across 11 distinct tools, plus the Zephyr SDK and two
source builds. The same loop is every contributor's first hour, not only a
runner's.
