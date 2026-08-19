---
id: 686
title: "`multi_pkg_workspace_freertos`'s node pkgs declare no `[package.metadata.nros.node]`, so the planner emits a stub run_plan and the fixture proves only that the ELF links"
status: resolved
type: bug
area: testing/orchestration
related: [issue-0683, phase-330]
---

## Symptom

The `freertos_firmware` build-fixture emits the Placeholder stub instead of a
real `run_plan.rs`:

```
nros-build: codegen skipped: nros-build: planner failed
  planning failed with 2 error(s):
  missing-source-metadata: missing source metadata for talker_pkg/talker
    [package=talker_pkg instance=talker_pkg.talker.0]
  missing-source-metadata: missing source metadata for listener_pkg/listener
    [package=listener_pkg instance=listener_pkg.listener.0]
```

`freertos_firmware_entry` passes anyway: it asserts the thumbv7m ELF builds,
never the emitted body. So the fixture has been proving that the image LINKS,
not that its launch file PLANS — which is the property its own doc-comment
claims ("drives the codegen library to emit `$OUT_DIR/run_plan.rs`").

Newly visible: issue 0683 moved this Entry pkg under `src/` and gave it the
`package.xml` it never had, so `nros sync` resolves a SystemModel and the
planner is reached for the first time. Before that it failed earlier, for a
different reason, and the stub looked the same.

## Cause

The planner synthesises a component artifact per node from the node package's
`[package.metadata.nros.node]` table, and matches it to the launch entry by
`package` + `executable` (`find_source_metadata` / `metadata_matches`,
`planner.rs:3129`). `nros-build` passes `metadata_files: Vec::new()`, so that
table IS the source.

The sibling fixture that works declares it:

```toml
# o5_nav2_compat_smoke/src/primary_node/Cargo.toml
[package.metadata.nros.node]
class = "primary_node::Primary"
name = "primary"
default_namespace = "/"
```

`multi_pkg_workspace_freertos`'s `talker_pkg` and `listener_pkg` have no such
table. Both use the same `nros::declarative_component!` + `nros::node!` pair in
their sources, so the packages look equivalent — the difference is entirely in
the manifest, and nothing reads the sources to notice.

## Fix

Three changes, in the order they were forced:

1. **`[package.metadata.nros.node]` on `talker_pkg` and `listener_pkg`**, with
   `class`/`name` matching the `<node pkg exec name>` entries in the launch
   file. The planner then synthesises both component artifacts and codegen
   emits the real body:

   ```rust
   pub fn run_plan_register_dispatch(executor: &mut ::nros::Executor<'static>) -> … {
       ::talker_pkg::register_dispatch(executor)?;
       ::listener_pkg::register_dispatch(executor)?;
   ```

2. **An `nros` dependency on the Entry pkg.** The real emit names
   `::nros::Executor`; the firmware crate never declared `nros` because the only
   body it had ever compiled was the stub, which references `::nros_platform`
   alone. So the first successful codegen broke the build — a latent gap the
   stub had been hiding, not a new one.

3. **The test fails on a stub.** Its stub arm was
   `eprintln!("build smoke verified")` followed by a fall-through to green,
   which is what made all of this invisible: the fixture reported success while
   exercising no codegen. It now asserts the emit is not a Placeholder and
   quotes the `// reason:` line (issue 0683) when it is.

Point 3 is the load-bearing one. Points 1 and 2 fix today's break; point 3 is
why it took months to notice, and would have caught it in a day.

## Verified

`freertos_firmware_entry` PASS on a real emit; `nav2_compat` and
`board_agnostic_run_plan` still PASS (3/3 together);
`check-leaf-lockfiles` green.
