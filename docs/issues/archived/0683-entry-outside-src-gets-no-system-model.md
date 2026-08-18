---
id: 683
title: "An Entry package outside `src/` never gets a SystemModel, so two compile-check fixtures emit a stub — and the skip blamed a tool that was installed"
status: resolved
type: bug
area: testing/orchestration
related: [phase-330, rfc-0060]
---

## Symptom

`nav2_compat::n11_launch_xml_ros2_compat_smoke` and
`board_agnostic_run_plan::board_agnostic_run_plan_links_against_any_board`
both skip, because their build fixture emitted the Placeholder stub instead of
real codegen. Neither has run for as long as that has been true.

## The stated reason was wrong, and had been for a while

The skips said "play_launch_parser absent at build time" and "nros-build
codegen unavailable at build time". Both describe an offline CI that no longer
exists: `play_launch_parser` is installed and on `PATH`, and RFC-0060 made the
resolver a LINKED library rather than a shelled binary. The real error was in
cargo's captured build-script stderr, which nothing surfaced:

```
nros-build: no SystemModel for `…/nav2_compat_smoke/demo_entry/launch/system.launch.xml`
at `…/nav2_compat_smoke/demo_entry/config/system_model.yaml`.
The model is BUILD OUTPUT (phase-330) … run `nros sync` …
```

Two investigations went after a tool that was working. Fixed separately: the
stub now carries a `// reason:` line and both tests report it (see below), so a
future wrong guess is not available.

## Cause

`scripts/build/compile-check-fixtures.sh` DOES run `nros sync` in the staged
workspace — and sync resolves what it finds under `<ws>/src/`. In both fixtures
the Entry package is a SIBLING of `src/`, not a member of it:

```
nav2_compat_smoke/
  demo_entry/          <- entry, with launch/system.launch.xml
  src/primary_node/
  src/secondary_node/
```

So sync resolved `src/secondary_node`'s own `launch/secondary.launch.xml` into
`build/nros/models/secondary_node/system_model.yaml`, and the entry's launch
file — the one the test is about — was never resolved at all.
`nros-build` then looks it up through the sanctioned locator
(`model_location::resolve_model_path`), finds nothing, and the build script
falls back to the stub.

The layout is deliberate: `demo_entry/build.rs` overrides
`Options::workspace_root` precisely because the entry "sits one level shallower
(sibling of `src/`)". That worked while models were committed files; phase-330
made them build output resolved by a `src/`-walking sync, and nothing connected
the two.

## Why it stayed invisible

The fallback swallows EVERY error from `generate_run_plan_with` and writes a
stub that compiles. A test that skips on the stub therefore cannot fail, whatever
goes wrong — the "tests must fail on unmet preconditions" rule one level up,
where the precondition is manufactured by the build. Four fixture build scripts
share this shape (`o5_nav2_compat_smoke`, both `n_board_agnostic_run_plan`
entries, `multi_pkg_workspace_freertos`).

## Fix

All three Entry packages moved to the canonical `<workspace>/src/<entry>/`:

| fixture | was | now |
| --- | --- | --- |
| `o5_nav2_compat_smoke` | `demo_entry/` | `src/demo_entry/` |
| `n_board_agnostic_run_plan` | `posix_entry/`, `freertos_entry/` | `src/…` |
| `multi_pkg_workspace_freertos` | `firmware/` | `src/firmware/` |

The `exclude` lists, `manifest_dir` rows, `.gitignore` rules, sibling path deps
and test-side paths moved with them, and the `Options::workspace_root` overrides
are DELETED — `from_env`'s own `manifest.parent().parent()` is correct at the
canonical depth, which is what those overrides were working around.

Two of the three needed one more thing, and both are the same rule:
**`nros sync` resolves PACKAGES, so a launch file has to live in one.**

- `n_board_agnostic_run_plan` kept its shared launch file in a bare
  `launch/` dir at the fixture root — deliberate (both Entry pkgs must plan
  from the SAME file), but a directory is not a package. It is now
  `src/shared_bringup/`, a bringup package with `package.xml` + `launch/`,
  which says the same thing in the vocabulary the toolchain reads.
- `multi_pkg_workspace_freertos`'s `firmware/` had no `package.xml` at all. It
  cross-built happily and emitted a stub, so its ELF proved the image links, not
  that the launch file plans.

## Verified

`nav2_compat`, `board_agnostic_run_plan` and `freertos_firmware_entry` all PASS
against real codegen — the first two were skipping, the third was passing over a
stub.

## Left open, deliberately

`multi_pkg_workspace_freertos` now reaches the PLANNER and fails there:

```
planning failed with 2 error(s):
missing-source-metadata: missing source metadata for talker_pkg/talker …
missing-source-metadata: missing source metadata for listener_pkg/listener …
```

So its `run_plan.rs` is still a stub — but for a real, newly-visible reason
instead of a missing model, and its test passes either way because it only
asserts the ELF builds. That is a separate defect in the fixture's node
metadata, not this one; it is recorded here rather than chased, and it is
findable now because the stub carries its reason.
