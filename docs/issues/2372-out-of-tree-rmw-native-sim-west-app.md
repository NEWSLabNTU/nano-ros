---
id: 2372
title: "The out-of-tree RMW acceptance image is not yet built on `board = \"native_sim\"` through `west_app` (RFC-0071 D16 step 3)"
status: open
type: test-gap
area: rmw, zephyr, build, cli
related: [2295, phase-545, rfc-0071]
found: 2026-11-09
---

**Home: [phase-545](../roadmap/phase-545-rmw-provider-scan-build-road.md).**
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
