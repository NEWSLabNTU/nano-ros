---
id: 683
title: "An Entry package outside `src/` never gets a SystemModel, so two compile-check fixtures emit a stub — and the skip blamed a tool that was installed"
status: open
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

## Directions

Candidates, not a plan.

- **Make sync see entries outside `src/`.** Most direct; the question is whether
  a bringup outside `src/` is a layout the CLI means to support, or whether
  these fixtures are the only ones shaped this way.
- **Move the entries under `src/`.** Canonical layout, and the `workspace_root`
  override in both build scripts goes away with it. Costs whatever the shallower
  layout was meant to exercise — which is not written down anywhere.
- **Set `NROS_MODEL_DIR` for these fixtures** in the compile-check builder. The
  narrowest fix, and the error message already suggests it; it leaves the
  underlying "sync does not see this package" gap in place for the next layout.
- **Stop the fallback from hiding failures** — make the stub arm fail the build
  for fixtures whose whole purpose is asserting codegen output. This turns two
  silent skips into two honest reds, so it wants to land WITH one of the above,
  not before.
