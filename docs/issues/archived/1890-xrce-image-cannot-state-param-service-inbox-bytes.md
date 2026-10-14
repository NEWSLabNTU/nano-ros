---
id: 1890
title: "A Zephyr XRCE image could not state `CONFIG_NROS_PARAM_SERVICE_INBOX_BYTES`, the knob XRCE's own refusal names"
status: resolved
type: bug
area: [zephyr, kconfig, xrce]
severity: low
found: 2026-10-14
resolved: 2026-10-14
related: [1352, 1485, 1722, phase-461, phase-484]
---

## Measured

The knob registry row for `PARAM_SERVICE_INBOX_BYTES`
(`packages/tooling/nros-zephyr-build/knobs.toml`) carried
`kconfig_if = "NROS_RMW_ZENOH"`, so the generated `zephyr/Kconfig` declared
`NROS_PARAM_SERVICE_INBOX_BYTES` only inside the zenoh `if`. On an XRCE image
the symbol does not exist, and a `.conf` line setting it is dropped with a
Kconfig warning.

But the knob is not zenoh's. Two other readers take it:

- `nros-node/build.rs` reads it on every backend. It is the stated size that
  the const assert in `param_sizing.rs` checks against the declared parameters.
- `nros-rmw-xrce-cffi/build.rs` binds it as `XRCE_PARAM_REQUEST_BYTES`
  (`xrce-config.txt`, issue 1722). It raises
  `NROS_XRCE_SERVICE_REQUEST_BUFFER_SIZE` to fit, and the refusal for an
  underived parameter request tells the user to state
  `CONFIG_NROS_PARAM_SERVICE_INBOX_BYTES`. That is a symbol the same image
  cannot declare.

## Fix

The `kconfig_if` is gone from the `BYTES` row, so the symbol is declared for
every backend. Its default is still `-1`, and the cmake forwarding
(`nros_cargo_build.cmake`) forwards nothing for `-1`, so an image that does not
state it is unchanged.

`PARAM_SERVICE_INBOX_DEPTH` stays under `NROS_RMW_ZENOH`. nros-node allocates
the depth's ring only on a backend that `SUPPORTS_CALLER_INBOX`
(`PARAM_INBOX_COUNT` is 0 otherwise), and XRCE reads no depth.

Found while closing phase-484's census follow-ups.
`scripts/check/config-knob-census.py` now also sees names read through a
`const NAME: &str = "NROS_…"` binding. It found 16 more build-time reads, none
of them a size.
