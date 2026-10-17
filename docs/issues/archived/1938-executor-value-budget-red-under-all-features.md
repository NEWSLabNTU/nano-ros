---
id: 1938
title: "The executor's byte-budget test is red under --all-features: signal-fd-wake and param-store add header fields with no named allowance"
status: resolved
type: bug
area: [node, executor, testing]
severity: low
found: 2026-10-17
related: [961]
---

## Measured

On `main` (`8f7e56feeb`):

```
cargo test -p nros-node --all-features --lib the_executor_value_does_not_scale
`Executor` is 2912 B at MAX_CBS=4 / MAX_NODES=4, over the 2835 B this value is budgeted;
a table that scales with a knob has come back inline (issue 0961).
```

No lane runs `nros-node --all-features`, so nothing reports it.

Each feature on its own over `--no-default-features --features std,alloc`
(value / ceiling, with a temporary print beside the assert):

| features | value | ceiling | headroom |
|---|---|---|---|
| `std,alloc` | 4856 | 4921 | 65 |
| `+rmw-cffi` | 2312 | 2321 | **9** |
| `+rmw-cffi,signal-fd-wake` | 2352 | 2321 | **-31** |
| `+param-store` | 4944 | 4961 | 17 |
| `+param-services` | 4944 | 4961 | 17 |
| `+sim-time` | 4896 | 4955 | 59 |
| `+scheduler-os-priority` | 4864 | 4921 | 57 |
| `+lifecycle-services` | 4864 | 4929 | 65 |
| every other feature | 4856 | 4921 | 65 |

Dropping any single feature from `--all-features` leaves it red (69-77 B
over), so it is not one field.

## Why

The test is a ceiling of 1280 B plus NAMED allowances for header fields that
do not scale with `MAX_CBS` / `MAX_NODES`. Two features add fields with no
allowance:

- `signal-fd-wake` adds 40 B and has none, so on the `rmw-cffi` base, whose
  `CffiSession` leaves 9 B of headroom, it alone goes over.
- `param-store` / `param-services` add 88 B against a 40 B allowance; the
  other 48 B come out of the shared headroom.

`sim-time` (6 B) and `scheduler-os-priority` (8 B) take smaller bites the same
way. Together they exceed the 9 B the `rmw-cffi` base leaves.

## Fix direction

Name each field's allowance under its own `cfg`, as the test's comment asks
("named rather than folded into the base"), after checking that none of them
scales with a knob. Then run the test under `--all-features` in a lane, so
the next field added under a feature nobody's lane enables gets reported.

## Resolution (2026-10-17)

Named allowances in `executor/storage.rs` for every field the measurement
found unnamed, each checked to scale with no knob:

- `signal-fd-wake`'s `Option<WakeSignalFd>` (an fd, one `Arc`, a task handle);
- `param-store`'s `param_store_refused` flag and `DeclaredParams` (two
  `&'static` slices into the entry's baked tables);
- `scheduler-os-priority`'s policy hook (one optional fn pointer), which the
  pool's allowance did not cover.

The test now passes under `--all-features`, `std,alloc,rmw-cffi,signal-fd-wake`,
`std,alloc,param-services` and `std,alloc`. What remains un-named is padding
(`sim-time`'s 6 B), which the base absorbs.

`node-std-tests` now runs this one test under `--all-features`, through its
`ran_tests` guard, so a field added under a feature no other build enables is
reported by the lane rather than by whoever next runs `--all-features`.
