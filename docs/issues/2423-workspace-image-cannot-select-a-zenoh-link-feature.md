---
id: 2423
title: "A workspace image cannot turn on a zenoh link feature (`link-tls`) without a node package naming zenoh"
status: open
type: limitation
severity: low
area: [cli, build, zenoh, examples]
related: [phase-535, rfc-0121, rfc-0122, rfc-0031, 1509]
found: 2026-11-10
---

**Home: [phase-535](../roadmap/phase-535-example-reorg-and-cadence-ci.md) W6.**
The `link-tls` rows of `examples/linux/rust/{talker,listener}` are each a
`linux_tls` image of that example's own one-node workspace
(`node_features = ["link-tls"]`, RFC-0121 D9), where
`nano2nano::test_tls_talker_listener_communication` resolves them. That needs
no enabler: `check-node-package-invariance` scopes `examples/workspaces/*` and
the templates, and exempts the per-platform examples, which exist to show
platform integration. What remains is the LANGUAGE workspace: a shared node
package there may not forward to `nros-rmw-zenoh`, so the pair cannot be a
`matrix::Config` TLS cell without image-selected backend features.

## What

On a single-package leaf, TLS is a feature of the leaf's own package:
`link-tls = ["dep:nros-rmw-zenoh", "nros-rmw-zenoh/link-tls"]`. That is where
`packages/api/nros/Cargo.toml` says zenoh link tuning belongs ("set on the
consumer's own `nros-rmw-zenoh` dep"), since `nros` dropped its `link-tls`
forward in phase 248.

A workspace image has no package of its own. It reaches its build three ways,
and none of them carries a backend link feature:

- `[image.<id>] node_features` (RFC-0121 D9) turns on a feature of a node
  package. A node package that forwards to `nros-rmw-zenoh` names an RMW, so
  `check-node-package-invariance` (issue 1509) refuses it, and the gate's
  baseline may only shrink. We measured this on phase-535 W6's first try: the
  `link-tls` forward in `talker_pkg` and `listener_pkg` built, and
  `test_tls_talker_listener_communication` passed on the images, but the fast
  line went red on 1509.
- `[image.<id>] features` lowers registry capabilities
  (`cargo_nano_ros::capability_resolver::CAPABILITIES`). There is no `link-tls`
  capability.
- `[image.<id>] env` / `knobs` reach build scripts, not cargo features.
  zpico-sys's TLS link is a cargo feature (`zpico-sys/link-tls`), not an env
  knob.

## Options (not chosen)

- **(a) A `link-tls` capability.** Add a `CAPABILITIES` row whose
  `backend_feature` is `link-tls`, declared by the zenoh backend in its
  `nros-rmw.toml` `[rmw.capabilities]`. A row's `nros_feature` is
  mandatory today, so this either brings back an `nros/link-tls` umbrella
  feature (a core-crate change, reversing the phase-248 removal) or makes
  `nros_feature` optional for a backend-only axis (a CLI change to the
  lowering and its C/C++ slots). Cost: the capability vocabulary grows a
  transport-link axis, and the C/C++ road needs a `c_define` / `cmake_token`
  answer, or an explicit "Rust only". It keeps node packages RMW-agnostic,
  and every board that forwards backend capabilities gets it for free.
- **(b) Board or entry features on the image.** Give `nros-board-linux` a
  `link-tls` forward, and add an image key that turns on features of the
  generated entry's board or backend dependency, for example
  `[image.<id>] backend_features = ["link-tls"]`. Cost: a new `system.toml`
  key, plus its schema, validation and row-agreement check (the shape
  `node_features` has in `fixtures-manifest.py validate-workspaces`), and a
  per-board forward for every board that should offer it. It is narrower
  than (a), and it puts an RMW-specific word in `system.toml`, which is
  where the RMW is already declared.

## Decision

**Question:** how does a workspace image turn on a backend link feature
(`link-tls` first) without a node package naming the RMW?

### Measured (origin/main, 2026-11-12)

- `CAPABILITIES` (`packages/cli/cargo-nano-ros/src/capability_resolver.rs`)
  has 6 rows. `nros_feature` is a required `&'static str` ("Always
  present"), and `backend_feature`, `c_define` and `cmake_token` are already
  `Option`. A backend-only axis therefore needs `nros_feature` to become
  `Option` too. It is a CLI-only type change, with no core-crate feature
  added.
- Which backend supports a capability is already the BACKEND's statement:
  `[rmw.capabilities]` in `nros-rmw.toml`, read through
  `ResolvedRmwIn::declares_capability`. Zenoh declares `safety`,
  `zero-copy-receive` and `alloc` there today.
- zenoh's link features: `zpico-sys` has 7 (`link-ip`, `-raweth`, `-tls`,
  `-can`, `-isotp`, `-ivc`, `-custom`). `nros-rmw-zenoh` forwards 4 of them
  (`link-ip`, `-tls`, `-custom`, `-isotp`). Whatever is chosen here
  generalises to those 4.
- The board road already has the forward shape: 8 of the 23 board packages
  carry a `safety-e2e` forward, including
  `nros-board-linux = ["nros-rmw-zenoh?/safety-e2e"]`. One board,
  `nros-board-mps2-an385`, already has a `link-tls` forward.
- The C/C++ road has no TLS at all today: there is no `tls` feature in
  `nros-c`, `nros-cpp` or `cmake/`. Either option is Rust-only for now,
  stated explicitly (`c_define`/`cmake_token` = `None`).
- What is blocked is small: the two `linux_tls` rows in
  `examples/linux/rust/{talker,listener}` work, and
  `test_tls_talker_listener_communication` passes on them. The gap is only
  that they cannot become a `matrix::Config` cell.

### Options

- **(a) A backend-only capability (recommended).** Make `nros_feature`
  optional, add a `link_tls` row (`backend_feature = "link-tls"`, no C/C++
  token), have zenoh declare it in `[rmw.capabilities]`, and add a
  `link-tls` forward to `nros-board-linux`. The image writes
  `features = ["link-tls"]`.
  - *UX:* the same key a user already uses for `safety`, with no new
    vocabulary in `system.toml`.
  - *Maintainability:* reuses the lowering, the backend declaration and the
    board-forward paths. A third-party backend offers its own link features
    the same way without a core edit.
  - *Design:* the capability vocabulary grows a transport-link axis that is
    backend-only. The cost is the `nros_feature` `Option`.
- **(b) An image key, `backend_features = [...]`.**
  - *UX:* explicit, but it is a third feature key beside `features` and
    `node_features`, and the user must know backend crate feature names.
  - *Maintainability:* a new key with its own schema, validation and
    `validate-workspaces` row-agreement check, plus the same per-board
    forward as (a).
  - *Design:* a raw passthrough of backend crate feature names, so nothing
    checks the backend actually offers one before cargo does.
- **(c) Leave it.** Keep the two per-example `linux_tls` rows. They are green.
  - *UX:* unchanged; TLS remains a single-package-leaf feature.
  - *Maintainability:* zero change. Two example rows stay outside the
    matrix, and the 3 other forwarded zenoh link features have no workspace
    road either.
  - *Design:* the gap stays documented here.

### Recommendation

**(a).**
- It reuses the backend-declared capability path, which already exists so
  that core never names a backend.
- It adds no `system.toml` key.
- It covers all 4 forwarded zenoh link features with one row each.

(c) is acceptable if the user prefers no change at severity low. (b) costs
more than (a) and checks less.

## Done when

`examples/workspaces/rust/src/pubsub_bringup` can declare
`linux_{talker,listener}_tls` images without a node package naming zenoh. At
that point the two example `linux_tls` rows can retire into a
`matrix::Config` TLS cell, the way phase-535 W6 moved the peer-mode rows.
