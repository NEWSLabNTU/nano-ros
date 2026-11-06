---
id: 2295
title: "`nros build` selects an RMW only from the compile-time in-tree table (`resolve_rmw`), so an out-of-tree backend the provider scan finds cannot be selected"
status: open
type: bug
area: cli, rmw, build
related: [rfc-0071, rfc-0094, rfc-0087, phase-439, phase-537, 1214]
found: 2026-11-06
---

**Home: none yet.** Reported by the phase-537 W12 book session with an
in-tree CLI. This is issue 1214's shape on the CLI build road. 1214 closed
the cmake road only.

## Symptom

A workspace holds an out-of-tree RMW provider:
`src/acme_rmw/package.xml` with `<nano_ros_provides kind="rmw" name="acme"/>`
and an `nros-rmw.toml`. `nros dev rmw dispatch acme` resolves it through
the provider scan. An image that declares `rmw = "acme"` and builds with
`nros build` is refused as an unknown RMW. RFC-0071 D5's acceptance test
("a fifth backend added out-of-tree with zero core edits") passes on the
cmake road and fails on the `nros build` road.

## Code site

`packages/cli/cargo-nano-ros/src/rmw_resolver.rs` has two lowerings:

- `resolve_rmw(declared)` reads `RMW_ROWS`, which `build.rs` bakes from
  `packages/rmw/` at compile time. Its set of names is the contents of one
  checkout.
- `resolve_rmw_in(&ScanResult, declared)` reads the winner's descriptor at
  selection time. It was added by phase-439 W4 for issue 1214.

Only `cmd/ws.rs` `run_rmw_dispatch` (the cmake seam) calls
`resolve_rmw_in`. Every `nros build` and `nros sync` site calls
`resolve_rmw`:

- `nros-cli-core/src/orchestration/facade.rs`: `derive()` (the selection
  facade `cmd/build.rs` writes through `facade::write_facade`) and its
  extra-backend loops. Also `.filter_map(|b| resolve_rmw(b).ok())`, which
  drops an unresolvable name silently.
- `nros-cli-core/src/orchestration/nros_config.rs` (entry system rmw).
- `nros-cli-core/src/cmd/codegen_system.rs` (`rmw_token`).
- `nros-cli-core/src/builder/west_app.rs` (west road).
- `nros-cli-core/src/orchestration/rmw_config.rs`,
  `orchestration/bridge_gen.rs`.

Issue 1214's "Not fixed here" section defers the board axis only. It
treats the rmw axis as done. That holds for cmake and not for the CLI's
own build road.

## Fix direction

- One selection-time lowering for the CLI. The build and sync sites take
  the workspace's `ScanResult` (the one `nros build` already computes for
  `--packages-up-to` / provider discovery) and call `resolve_rmw_in`.
  `resolve_rmw` is kept only where no workspace exists. Better still, the
  in-tree table becomes the scan's in-tree root, so no second answer exists.
- `ResolvedRmwIn` returns the provider's directory. The facade needs it to
  path-dep the backend crate (`dispatch.rlib_dep`), which a `'static`
  in-tree table cannot express for an out-of-tree crate.
- Replace the silent `filter_map(... .ok())` in `facade.rs` with a refusal
  that names the backend.
- Acceptance: a test workspace with an out-of-tree `acme` RMW provider
  builds an image with `rmw = "acme"` through `nros build`. This is 1214's
  acceptance test, run on this road. Add a gate that lists the
  `resolve_rmw(` call sites in `nros-cli-core` and refuses a new one that
  has a workspace in scope.
