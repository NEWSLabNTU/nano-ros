---
id: 2295
title: "`nros build` selects an RMW only from the compile-time in-tree table (`resolve_rmw`), so an out-of-tree backend the provider scan finds cannot be selected"
status: resolved
type: bug
area: cli, rmw, build
related: [rfc-0071, rfc-0094, rfc-0087, phase-439, phase-537, phase-545, 1214]
found: 2026-11-06
resolved: 2026-11-09
resolved_in: "phase-545 W1-W3: every CLI site resolves an RMW name through the provider scan (RmwIndex); the generated entry links and registers a backend the board does not name; the out-of-tree `acme` fixture builds and runs through `nros build`; gate check-rmw-resolve-sites"
---

**Home: [phase-545](../../roadmap/phase-545-rmw-provider-scan-build-road.md), implementing [RFC-0071 Amendment 2026-11-07](../../design/0071-rmw-backend-descriptor.md#amendment-2026-11-07--nros-build-selects-an-rmw-through-the-provider-scan-issue-2295)
(D14–D16), accepted 2026-11-07 (user ruling), implemented by
[phase-545](../../roadmap/phase-545-rmw-provider-scan-build-road.md).** Resolved
by phase-545 W1–W3 (2026-11-09); see "Resolution" below. Reported by the phase-537 W12 book session with an in-tree CLI. This is issue 1214's shape on the CLI build road. 1214 closed
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
`resolve_rmw_in`. Every `nros build` and `nros configure` site calls
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

## Resolution (phase-545 W1–W3, 2026-11-09)

- **W1 (RFC-0071 D14).** Every CLI site resolves an RMW name through an
  `RmwIndex` over the provider scan (`orchestration::rmw_index`), root 0
  narrowed to `packages/rmw/` (`InTreeScope::Rmw`). The baked table and
  `resolve_rmw` / `known_rmw` / `canonical_rmw` are `#[cfg(test)]`: the
  compile-time convention check and the oracle that holds the table equal to
  the scan. The silent `filter_map(… .ok())` in `facade.rs` is a refusal
  naming the backend and the image.
- **W2 (D15).** A backend the board crate does not name is linked by the
  generated entry (a direct path dependency on `[rmw.provides.cargo] crate`
  at the provider's directory) and registered through the
  `__nros_register_backends!(image)` arm.
- **W3 (D16).** `cmd/build.rs`'s entry generation was the last site on the
  SDK root alone (`facade::entry_selection`, the W1 interim adapter); it now
  takes the workspace, so `nros build` finds a workspace provider. The
  fixture workspace `packages/testing/nros-tests/fixtures/out-of-tree-rmw/`
  (an `acme` provider delegating to zenoh, a talker, a `board = "linux"`,
  `rmw = "acme"` image) is this issue's acceptance test:
  `nros-cli-core/tests/out_of_tree_rmw_build.rs` (step 1, `check-cli-tests`)
  and `nros-tests/tests/out_of_tree_rmw_e2e.rs` (step 2, row
  `workspace-out-of-tree-rmw`, against a router). Running it found one more
  gap: `acme` links zenoh, whose hosted `.init_array` ctor registers `zenoh`
  too, and the `linux` board dropped the baked `rmw` selector, so the open
  was `Ambiguous`. `nros::main!` now bakes the image's `rmw` for an image
  whose entry registers its own backend, and the `linux` board reads it
  (below `$NROS_RMW`, as the RTOS boards already did).
- **Gate.** `check-rmw-resolve-sites` (fast lane) refuses the free
  `resolve_rmw(`, `RMW_ROWS` and the static capability helpers in
  `nros-cli-core/` and `cargo-nano-ros/src/`. No allowlist; the only
  exceptions are `build.rs` and the resolver's own `#[cfg(test)]` items.
- **Not here.** D16 step 3 (the same image on `board = "native_sim"` through
  `west_app`) is a follow-up by user ruling:
  [issue 2372](2372-out-of-tree-rmw-native-sim-west-app.md).
