---
id: 686
title: "`multi_pkg_workspace_freertos`'s node pkgs declare no `[package.metadata.nros.node]`, so the planner emits a stub run_plan and the fixture proves only that the ELF links"
status: open
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

## Fix direction

Add `[package.metadata.nros.node]` to `talker_pkg` and `listener_pkg` with the
class/name/namespace matching what their launch file declares, then confirm the
emitted `run_plan.rs` carries `::talker_pkg::register` and
`::listener_pkg::register`. Cheap, and the shape is already established by the
nav2 fixture.

Worth doing at the same time, because it is why this sat unnoticed:
**`freertos_firmware_entry` should assert the emitted body**, not just the ELF.
A codegen fixture whose test never reads the codegen output cannot fail for the
reason it exists. If asserting the body is genuinely out of scope for the QEMU
lane, the test should at least fail — not pass — on the Placeholder stub, the
way `nav2_compat` and `board_agnostic_run_plan` skip on it.

## Related

The stub now carries a `// reason:` line (issue 0683), which is how this was
identified at all — the previous stub recorded nothing, and the consuming test
asserted a cause its author had guessed years earlier.
