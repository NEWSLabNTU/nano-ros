---
id: 2372
title: "The out-of-tree RMW acceptance image is not yet built on `board = \"native_sim\"` through `west_app` (RFC-0071 D16 step 3)"
status: resolved
type: test-gap
area: rmw, zephyr, build, cli
related: [2295, phase-545, rfc-0071]
found: 2026-11-09
resolved: 2026-11-10
resolved_in: "the acme image builds on Zephyr through west_app (row workspace-zephyr-out-of-tree-rmw) and publishes through acme; its Kconfig choice is the descriptor's `[rmw.zephyr] module_backend` (RFC-0071 D18) and the Zephyr arm selects the declared backend"
---

**Home: [phase-545](../../roadmap/phase-545-rmw-provider-scan-build-road.md).**
Split out of phase-545 W3 by user ruling (2026-11-09): RFC-0071 D16's step 3
is a follow-up, not part of that phase, because it needs the Zephyr SDK on
the lane that runs it.

## What is missing

RFC-0071 D16 names three acceptance steps for "a backend added out-of-tree
with zero core edits" on the `nros build` road. Phase-545 W3 landed steps 1
and 2 over the fixture workspace
`packages/testing/nros-tests/fixtures/out-of-tree-rmw/` (an `acme` RMW
provider in `src/acme_rmw`, delegating to zenoh):

1. `nros build` configures and generates the entry for `board = "linux"`,
   `rmw = "acme"` (`nros-cli-core/tests/out_of_tree_rmw_build.rs`);
2. the built image runs against a router and publishes through `acme`
   (`nros-tests/tests/out_of_tree_rmw_e2e.rs`, row
   `workspace-out-of-tree-rmw`).

Step 3 is not covered: the same image with `board = "native_sim"` built
through `builder/west_app.rs`. That is the step that proves D15's crate
directory rule on the west road (ruling 2: on an image with a west half, the
west build's backend dependency IS D15's dependency, and `west_app`'s deleted
`backend_dir` walk is replaced by `ResolvedRmwIn::package_dir`). W2's unit
tests cover `west_app::resolve` for an `acme` provider; no build does.

## Fix direction

- Add `[image.native_sim]` (`board = "native_sim"`, `rmw = "acme"`) to the
  fixture's bringup. `acme_rmw` then needs a `platform-zephyr` feature
  forwarding to `nros-rmw-zenoh/platform-zephyr`, and an
  `[rmw.provides.cargo.platform_features]` row for it.
- A west fixture row (the Zephyr lane's, not the native one), and a test on
  that lane that asserts the west entry depends on `acme-rmw` at the
  provider's directory and that the image boots on `acme`.
- On Zephyr there is no hosted `.init_array` self-registration, so the
  baked-selector rule phase-545 W3 added for hosted images (an image whose
  entry registers its own backend bakes `rmw` as its selector) should be
  checked on that arm too, not assumed.

## Resolution

Resolved 2026-11-10. The image is `[image.zephyr]` with `board = "zephyr"`,
not `[image.native_sim]` with `board = "native_sim"`. Since issue 1519 an image
names the nano-ros board id, and the `zephyr` board's `[board.zephyr]
west_board` is `native_sim/native/64`. The image id `native_sim` is refused by
`check-host-platform-vocabulary` (phase-500: a `native…` image id reads as a
host image).

- **What failed.** `west_app` already resolved `acme` through the scan and
  depended on `acme-rmw` at its package directory. The west configure then
  failed twice, both times on a closed list of RMW names. The module hook's
  Kconfig render refused `acme` ("has no Zephyr backend"), and
  `nros_check_image_agreement` would have refused Kconfig's `zenoh` as a
  disagreement with `acme`.
- **The fix (RFC-0071 D18).** A backend's descriptor states the Zephyr module
  backend it is built over, `[rmw.zephyr] module_backend = "zenoh" | "xrce" |
  "cyclonedds"`. The three in-tree Zephyr backends state their own name, and
  acme states `zenoh`. The Kconfig render reads it through the provider scan,
  and a backend that states none is refused naming the line to add. The image
  facts carry it as `NROS_IMAGE_ZEPHYR_MODULE_BACKEND`, which the agreement
  check compares against. The descriptor is now a configure input of the
  fragment (`NROS_LEAF_KCONFIG_INPUT`).
- **The baked-selector question, measured.** The Zephyr arm got neither half
  of phase-545 W3's rule. It renders no `(image)` call, so nothing was baked,
  and it opens through `Context::baked()`, which never reads
  `NROS_BOOT_CONFIG`. What registers on the image: acme, through the entry's
  `(gated)` arm, and then `zenoh`, through the module's
  `nros_app_register_backends()`. The module emits that hook for
  `CONFIG_NROS_RMW_ZENOH`, and `nros_rmw_zenoh_register` is defined in the
  image (`nm zephyr.exe`: `T nros_rmw_zenoh_register`). The first build booted
  on acme only because `NROS_RMW_MAX_BACKENDS` is derived as 1 and the gated
  arm runs first, so zenoh's registration failed silently. The Zephyr arm now
  applies the image's declared `rmw` to its executor config
  (`config.rmw(..)`), so the open selects acme by name. A Zephyr image's `rmw`
  must be the canonical name, and an alias is refused at the Kconfig render.
- **Logging.** acme logs its marker through `nros_log`, which on native_sim
  lands on Zephyr's logger (`[INFO] nros: … acme rmw: publishing through
  backend acme`). There is no `println!` in the image (issue 0589).
- **Tests.** `nros-cli-core/tests/out_of_tree_rmw_build.rs`
  (`nros_build_generates_a_west_application_over_a_workspace_rmw_provider`)
  runs a `--dry-run` with no Zephyr SDK. It asserts the west application's
  `acme-rmw` path, `platform-zephyr` and `rmw-acme` gating, and the `(gated)`
  arm, and that the module hook's fragment says `CONFIG_NROS_RMW_ZENOH=y`.
  Row `workspace-zephyr-out-of-tree-rmw` builds the image on the Zephyr lane,
  and `nros-tests/tests/out_of_tree_rmw_zephyr_e2e.rs` runs it against a
  router and a native listener. There are also unit tests in `leaf_kconfig`,
  `image_facts`, `rmw_descriptor` and `nros-macros`.
- **Found on the way:** [issue 2393](2393-west-app-backend-dep-no-edition-feature.md).
  The west application's backend dependency carries no ROS edition feature.
