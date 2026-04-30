# nros-rmw-uorb

PX4 uORB RMW backend for nano-ros. Wires nros pub/sub onto PX4's uORB
in-process message bus, so a nano-ros node compiled into a PX4 module
can talk to other PX4 modules at process-local cost.

## Role

Implements the [`nros-rmw`](../../core/nros-rmw) trait surface
(`Session`, `Publisher`, `Subscriber`) over PX4's uORB. Service +
client traits are placeholders — uORB is pub/sub-only; service-style
RPC is not yet supported (`90.4b` — deferred post-Phase-90 v1).

## Source layout

| File | Role |
|------|------|
| `src/lib.rs` | Public re-exports + `UorbRmw` factory. |
| `src/session.rs` | Module lifetime; spawns `nros_px4::run_async` driver. |
| `src/publisher.rs` / `src/subscriber.rs` | Typed-trampoline registry mapping nros-CDR payloads ↔ uORB message structs. |
| `src/service.rs` | Stubs (post-v1). |

## Naming

ROS topic name → uORB topic name lookup table is built at compile time
from a TOML file (`phf` perfect hash). The map is owned by
`nros-px4`, not this crate, since adding a new uORB topic requires a
typed trampoline registration.

## When to use

- **Inside PX4.** A nano-ros node that runs as a PX4 module under SITL
  or on Pixhawk hardware.
- Cross-process / cross-machine communication still uses zenoh / DDS
  / XRCE — `nros-rmw-uorb` is intra-process only.

## See also

- [Phase 90 — PX4 RMW + nros-px4](../../../docs/roadmap/phase-90-px4-rmw.md) (or `archived/` after closeout)
- [Phase 98 — PX4-Autopilot vendoring + SITL E2E](../../../docs/roadmap/phase-98-px4-vendoring.md)
- [`nros-px4` driver crate](../nros-px4)
- Source on GitHub: <https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/px4/nros-rmw-uorb>
