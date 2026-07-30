---
id: 315
title: "Three `<rmw>/` path levels survive in examples/ behind checker carve-outs; two of them encode deployment location, not RMW"
status: resolved
type: tech-debt
area: build
related: [issue-0314, issue-0295, issue-0319, rfc-0026, phase-316]
---

## Finding (2026-07-28)

RFC-0026 states the rule unconditionally:

> Phase 118 + 168 **collapsed the RMW dimension out of the path** … So one
> `examples/zephyr/rust/talker/` builds against zenoh, xrce, or cyclonedds —
> there are no `<rmw>/` siblings.

`scripts/check-example-matrix.sh` enforces it, but *with carve-outs*. After
issue 0314 removed every abandoned per-RMW tree, those carve-outs are the only
`<rmw>/` paths left in the repo:

| path | tracked | live refs | exemption |
| --- | --- | --- | --- |
| `zephyr/rust/cyclonedds/talker-aemv8r` | 12 | 10 | allowlist line |
| `zephyr/cpp/cyclonedds/talker-aemv8r` | 9 | 18 | allowlist line |
| `px4/rust/xrce/{offboard-companion,px4-probe,px4-stub}` | 16 | 13 | structural (whole platform) |
| `px4/cpp/uorb/nros-register-check` | 9 | 16 | structural (whole platform) |

("live refs" excludes `docs/roadmap/archived/`.)

## The px4 pair does not encode an RMW at all

The original defence (archived issue 0295) called px4's axis a "transport
integration case". That is close, but the repo's own docs say something
sharper. From `examples/px4/rust/xrce/README.md`:

> *"Unlike the `px4/.../uorb/` examples (in-firmware uORB), these run on the
> host or a peer MCU **beside** PX4."*

The axis is **where the code runs** — in-firmware versus companion — and the
backend follows from that rather than being chosen:

- **In-firmware** modules share PX4's own uORB bus. uORB skips serialization
  entirely, and that is the point: they demonstrate interop with existing PX4
  apps. They cannot be RMW-agnostic by construction.
- **Companion** nodes are ordinary nano-ros nodes. Their RMW is pinned to XRCE
  because that is what PX4's `uxrce_dds_client` speaks — dictated by the peer,
  not selected by us.

So the fix is not to flatten these but to **rename the level to the axis it
actually encodes**: `px4/cpp/firmware/…` and `px4/rust/companion/…`. Neither is
an RMW token, so the checker's px4 structural exemption deletes itself, and the
directory finally says what the code is instead of which backend it happens to
speak.

## The zephyr pair is a plain violation

`zephyr/{rust,cpp}/cyclonedds/talker-aemv8r` is distinguished by its **board**
(aemv8r), not its backend. It belongs at `zephyr/{rust,cpp}/talker-aemv8r`
under the existing variant-suffix convention (`-rtic`, `-async`, `-aemv8r`, …).
The allowlist comment concedes the shape was accidental:

> Both languages carve out — the rust sibling was **missed** when the cpp one
> landed (same single-board reference shape).

With both renames done, `allowed_roots` is empty and RFC-0026's rule is true as
written, with no exceptions to explain.

## Three complications found while scoping

**1. There is no uORB interop example to separate.** `nros-register-check` is
not a demo — its own header says so:

> *"Trivial PX4 module: on launch, call `nros_rmw_uorb_register()` and log the
> return code. The build itself is the validation."*

That is a link/registration assertion, and CLAUDE.md puts non-example binaries
under `packages/testing/`. So "separate the uORB interop examples" is really
two jobs: move the check out of `examples/`, and **write** the interop demo (a
nano-ros node exchanging with a stock PX4 app over uORB), which does not exist
yet.

**2. A uORB bridge cannot look like the existing bridges.**
`examples/bridges/` is already the right home and already states the rule — a
bridge "spans transport slots" and so "does not belong to a single cell". But
`tt-zenoh-to-xrce` and `tt-zenoh-to-cyclonedds` are POSIX Rust binaries, and
uORB is an **in-process** bus: a uORB bridge must be a C++ PX4 module. The
category still fits; the implicit "standalone POSIX binary" assumption does
not.

**3. PX4 already ships `uxrce_dds_client`,** which is precisely uORB →
XRCE-DDS. A nano-ros uORB→DDS bridge would duplicate it. The non-duplicative
framings are uORB→**Zenoh** (PX4 has nothing there), or a bridge whose point is
the multi-RMW registry in-firmware rather than the translation itself.
**Undecided** — this gates the bridge work item, not the renames.

## Also drifted

`examples/bridges/README.md` still describes the retired path form
(`<plat>/<lang>/<rmw>/<example>`). Same drift class as the rest of this issue.

## Plan

Sequenced in **phase-316**, which implements RFC-0026. The renames (W1–W3) are
independent of the undecided bridge question (W4), so they can land first.

## Acceptance

- No `examples/<plat>/<lang>/<rmw-token>/` path remains.
- `scripts/check-example-matrix.sh` passes with `allowed_roots` **empty** and
  the px4 structural exemption in `is_allowed()` deleted.
- RFC-0026 needs no "except" clause.
- `just check` and the example-fixture coverage test green.

## Resolved by phase-316 W1–W3 (2026-07-31)

All three levels are gone and `check-example-matrix.sh`'s allowlist is **empty**
— along with its `is_allowed()` px4 branch, so no future px4 case can inherit a
structural carve-out it never earned.

This issue's own reading was right and worth restating: two of the three levels
did not encode an RMW, so the fix was to RENAME rather than flatten — flattening
would have destroyed real information.

| was | is | what the level actually named |
| --- | --- | --- |
| `px4/rust/xrce/` | `px4/rust/companion/` | where the code RUNS — beside PX4, not in firmware. Its RMW is fixed by whatever `uxrce_dds_client` speaks, so it was never a choice, so it was never a variant axis. |
| `px4/cpp/uorb/` | gone → `packages/testing/nros-px4-register-check/` | nothing. The tree held one link-check module whose own header says the build IS the validation — not an example, so CLAUDE.md puts it under `packages/testing/`. |
| `zephyr/{cpp,rust}/cyclonedds/talker-aemv8r/` | `zephyr/{cpp,rust}/talker-aemv8r/` | a board, already stated by the `-aemv8r` suffix. |

Moving the register-check out also dissolved a two-file indirection nobody would
have chosen deliberately: PX4 requires
`<EXTERNAL_MODULES_LOCATION>/src/modules/<name>/CMakeLists.txt` and the example
tree required `<plat>/<lang>/<rmw>/<example>/`, so the real CMakeLists was
hoisted to the example path and a shim at PX4's path `include()`d it back. Two
layout rules over one directory, one served by an indirection. Outside
`examples/` only PX4's applies.

RFC-0026 now carries the table above, so a future reader does not "helpfully"
flatten `companion/` back into `px4/rust/`.

Filed on the way: **issue 0356** — `px4_e2e.rs` targets
`examples/px4/rust/uorb/{talker,listener}`, retired by phase-277 W7.
