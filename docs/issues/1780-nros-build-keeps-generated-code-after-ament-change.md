---
id: 1780
title: "`nros build` after an ament change (e.g. `apt install --only-upgrade
  'ros-humble-*'`) builds the OLD generated message code — no regeneration, a
  byte-identical binary, no diagnostic; only an explicit `nros sync` notices"
status: open
type: bug
area: cli, codegen, build
severity: high
found: 2026-10-10
related: [rfc-0104, phase-485, 1018, 1781]
---
## Measured

phase-485 M2, `examples/native/rust/talker` (cargo road), CLI built from
`origin/main` at the time. A scratch ament prefix carries a copy of the host's
`std_msgs` with `package.xml` 4.9.2 → 4.9.3 and a field added to `String.msg`
(`int32 m2_extra`); its `resource_index` entries are copied from the host so it
is a complete ament package. The prefix is appended LAST on
`AMENT_PREFIX_PATH` (first is shadowed by issue 1781).

| step | `generated/std_msgs` | field | binary |
| --- | --- | --- | --- |
| A — baseline `nros build` | `b68de0fc…` | no | `21d57619…` |
| B — ament changed, `nros build` | `b68de0fc…` | **no** | `21d57619…` **unchanged** |
| C — `nros sync`, then `nros build` | `2ea56b3e…` | yes | `4f7c81ec…` |
| D — ament restored, `nros sync` + `nros build` | `b68de0fc…` | no | rebuilt |

So `nros build` neither re-runs codegen nor detects that the inputs moved: the
image is built from generated code for an interface the host no longer has.
For a real ROS upgrade that changes a message, the result talks a different
wire layout from its ROS peers, and nothing says so.

## Why

The generated tree is the output of `nros sync`, and nothing records which
ament inputs it was generated FROM, so nothing can tell it is stale. Issue
1018's class (a configure-time emitter with no freshness edge) with the ROS
install as the input that moved.

## Fix (RFC-0104 D5, phase-485 W5)

Record `[generated.<pkg>] ament` — a hash of the interface files and
`package.xml` the package was generated from, plus the codegen version — and
have `nros build` re-run generation for a package whose hash moved (or refuse,
naming `nros sync`, if regeneration cannot be done in-build). Not yet measured
on the cmake and west roads; acceptance is this table's row B regenerating on
all three.
