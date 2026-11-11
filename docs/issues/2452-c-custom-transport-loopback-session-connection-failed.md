---
id: 2452
title: "The C custom-transport loopback example exits 1: its session over `custom://loopback` answers ConnectionFailed"
status: open
type: bug
severity: low
area: [examples, rmw-zenoh, testing]
related: [phase-535]
found: 2026-11-12
---

## Measured

`examples/linux/c/custom-transport-loopback`, built and run on linux (zenoh),
2026-11-12:

```
loopback: transport installed
[INFO] nros: boot config: domain 0 (default), locator custom://loopback (baked), rmw zenoh (baked)
[ERROR] nros: zpico Session -> ConnectionFailed (the two are indistinguishable downstream)
[nros] .../src/main.c:217 rclc_support_init_with_locator(&app.support, "custom://loopback", 0) -> -4
```

Exit 1. The same failure from both shapes of the example, so it is not the
phase-535 W6 migration: the pre-migration leaf (`origin/main` at `db80b10941`,
configured as a standalone CMake leaf) and the migrated one-package workspace
(`nros build native_c_custom_transport_loopback_bringup:linux`) print the same
lines.

The README promises exit 0 with all four callbacks fired. The session never
opens, so none of `write` / `read` / `close` is reached.

## Why nothing noticed

Its `examples/fixtures.toml` row is build-only (phase-275 W2: "UNVERIFIED ...
add a runtime assertion for it as follow-up"). The row compiles and links in
every lane; no lane runs the binary. That is the shape the `custom-platform`
row had before issue 1384 gave it a runtime cell.

## To do

- Find why the zenoh-pico session over the registered custom link refuses
  (a peer-less loopback has no router to answer the zenoh `InitSyn`; a
  loopback ring that echoes the client's own frames back may never produce
  an `InitAck`).
- Give the row a runtime consumer once it exits 0, so it cannot regress
  silently again.
