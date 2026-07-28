---
id: 315
title: "Three `<rmw>/` path levels survive in examples/ behind checker carve-outs; consolidate them into `<platform>/<language>/`"
status: open
type: tech-debt
area: build
related: [issue-0314, issue-0295, rfc-0026]
---

## Finding (2026-07-28)

RFC-0026 already states the rule, and states it unconditionally:

> Phase 118 + 168 **collapsed the RMW dimension out of the path** … So one
> `examples/zephyr/rust/talker/` builds against zenoh, xrce, or cyclonedds —
> there are no `<rmw>/` siblings.

`scripts/check-example-matrix.sh` enforces it, rejecting any
`examples/<plat>/<lang>/<name>/` whose `<name>` matches
`zenoh|xrce|dds|cyclonedds|uorb`. But it enforces it *with carve-outs*, and
after issue 0314 removed every abandoned per-RMW tree, the carve-outs are the
only `<rmw>/` paths left in the repo:

| path | tracked files | live refs | status |
| --- | --- | --- | --- |
| `zephyr/rust/cyclonedds/talker-aemv8r` | 12 | 10 | allowlisted |
| `zephyr/cpp/cyclonedds/talker-aemv8r` | 9 | 18 | allowlisted |
| `px4/rust/xrce/{offboard-companion,px4-probe,px4-stub}` | 16 | 13 | structural exemption |
| `px4/cpp/uorb/nros-register-check` | 9 | 16 | structural exemption |

("live refs" excludes `docs/roadmap/archived/`.)

The goal is to consolidate these into `<platform>/<language>/<example>` so the
rule holds without exceptions and the checker needs no allowlist at all. The
two groups are **not** equally settled, and the second is contested.

## Group 1 — zephyr cyclonedds: a plain layout violation

`zephyr/{rust,cpp}/cyclonedds/talker-aemv8r` is a single-board CycloneDDS
reference for aemv8r. The backend is not what makes it distinct — the *board*
is. Under the current rule it should be `zephyr/{rust,cpp}/talker-aemv8r`,
matching the existing `-aemv8r` variant-suffix convention that
`examples/README.md` already documents (`-rtic`, `-async`, `-embassy`,
`-aemv8r`, …).

The allowlist comment concedes the shape is accidental:

> Both languages carve out — the rust sibling was **missed** when the cpp one
> landed (same single-board reference shape).

A carve-out that had to be extended because someone forgot it is a carve-out
that is not carrying an argument.

**Work:** `git mv` both, then update ~28 referencing files —
`Cargo.toml`, `just/zephyr-setup.just`,
`packages/testing/nros-tests/tests/examples_fixture_coverage.rs`,
`scripts/check-example-matrix.sh` (drop both allowlist lines),
`book/src/getting-started/arm-fvp.md`, RFC-0026, and the phase-217 /
phase-275-276 roadmap notes. Mechanical, but the fixture-coverage test and the
book page both pin the path, so it is not a pure rename.

## Group 2 — px4: consolidating here reverses a deliberate decision

`px4/rust/xrce` and `px4/cpp/uorb` are exempt *structurally* — the whole
platform is, so new px4 transport cases need no carve-out line. That exemption
was argued, not defaulted. From archived issue 0295:

> For PX4 this is a **false positive**: px4's directory axis is the *transport
> integration case* (uORB vs XRCE — PX4's two native messaging surfaces), not
> the retired RMW axis. … The dir is correct; the carve-out list is stale.

The claim is that on PX4 these names denote something different from what they
denote elsewhere. Off-platform, `xrce` names a nano-ros RMW backend selectable
at build time. On PX4, uORB and XRCE are the airframe's own two messaging
surfaces, and an example targets one or the other as an integration case —
closer to a *platform* axis than an RMW axis.

Two facts make this more than semantics:

- **They are not the same example built two ways.** `px4/cpp/uorb` holds
  `nros-register-check` plus a `src/modules/nros_register_check/` PX4 module
  tree; `px4/rust/xrce` holds `offboard-companion`, `px4-probe` and `px4-stub`.
  Disjoint sets of programs, not one role under two backends. Flattening yields
  `px4/rust/{offboard-companion,px4-probe,px4-stub}` and
  `px4/cpp/nros-register-check` — which loses the statement of which surface
  each targets unless it moves into the name.
- **`px4/cpp/uorb` is not shaped like an example.** It carries a nested
  `src/modules/…/CMakeLists.txt` PX4 module tree, so it is not a standalone
  copy-out project in the sense `examples/README.md` defines. Moving it is not
  a rename; it needs a decision about what the copy-out contract means for a
  PX4 module.

So this half needs a decision recorded before any move:

1. Consolidate anyway, encoding the surface in the name
   (`px4-offboard-companion-xrce`, `nros-register-check-uorb`, …), accepting
   that 0295's distinction is real but preferring one layout rule with zero
   exemptions.
2. Keep the px4 exemption and instead make it *explicit in RFC-0026*, which
   currently states the no-`<rmw>/` rule with no exceptions — the RFC and the
   checker disagree today, and the checker is the one carrying the nuance.

Option 2 is cheaper and is what the code already does; option 1 is what makes
the rule true as written. Either is defensible, but the current state — an
unconditional RFC plus an undocumented structural exemption — is the worst of
both, because it is why the zephyr carve-out got extended by accident rather
than questioned.

## Fixed already (2026-07-28)

Small things found while scoping this, corrected in the same commit:

- **Two stale allowlist entries.** `examples/qemu-arm-baremetal/rust/dds` and
  `examples/qemu-esp32-baremetal/rust/dds` were still listed after issue 0314
  deleted them. Harmless (the allowlist only filters) but misleading — the same
  drift phase-277 W7 had already pruned once. Removed.
- **A dangling cross-reference.** The script cited
  `docs/issues/archived/0051` for the px4 exemption; `0051-*` is the
  deploy-target SSoT issue. The reasoning is in `archived/0295-*`. Both the
  file-path comment and the `issue #51` inline comment now point there.

## Acceptance

- No `examples/<plat>/<lang>/<rmw-token>/` path remains, **or** RFC-0026 states
  the px4 transport-axis exception explicitly and the checker's allowlist is
  empty of everything else.
- `scripts/check-example-matrix.sh` passes with no `allowed_roots` entries.
- `just check` and the example-fixture coverage test green.
