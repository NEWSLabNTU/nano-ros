---
id: 2133
title: "The Zephyr Rust XRCE talker cannot boot: its entity inventory no longer composes, so MAX_SUBSCRIBERS falls to the crate default and the session allocation is 284 KB"
status: open
type: bug
area: [zephyr, xrce, build]
severity: medium
found: 2026-10-29
related: [1033, 1796, 1808]
---

## Measured

The image is `build-rust-talker-xrce`, Zephyr native_sim, built from main
`e0f81b4ac5` on 2026-10-29. The cell
`zephyr::example_e2e::case_19_xrce_rust_pubsub_e2e` fails 3 of 3, both in the
sweep and alone: the listener receives 0 samples. The test does not print the
talker's console, and run by hand against `MicroXRCEAgent udp4 -p 2400` the
talker dies at boot:

```
nros: HEAP EXHAUSTED (TOO SMALL): request 284312 bytes, arena 131584 bytes, free 131024 bytes, ...
nros: PANIC platform heap exhausted
```

The caller address resolves into `xrce_session_create`, which is the
one-shot `calloc` of `xrce_session_state_t`. Issue 1033 established that
this allocation is sized by the XRCE pool caps at build time.

## Why

| image | `CONFIG_NROS_XRCE_MAX_SUBSCRIBERS` | source |
| --- | --- | --- |
| `build-c-talker-xrce` | 0 | stated in its `system.toml` |
| `build-rust-listener-xrce` | 1 | stated in its `system.toml` |
| `build-rust-talker-xrce` | **-1** | nothing |

`examples/zephyr/rust/talker/system.toml` deliberately does NOT state
`max_subscribers`. Its comment explains that this is the one standalone
Zephyr leaf whose entity inventory composes (`derived`, MAX_SUBSCRIBERS 0;
phase-481 W3, issue 1796). It also records exactly what deleting the row
did then: `HEAP EXHAUSTED (TOO SMALL): request 283912 bytes, arena 131584`.

Today that image's `nros/entity_inventory.cmake` reads:

```
no components were registered in this image, so there is nothing to compose.
```

So the derivation is refused, the knob stays `-1`, and the crate default
applies. The image cannot boot.

## Not yet known

- Which commit stopped the composition. Candidates touch the Zephyr Rust
  probe / sidecar road, for example phase-517 W5 `133d7fef1b` ("fresh probe
  sidecars state a contract-less image"). Not bisected.
- Whether other Rust Zephyr leaves that rely on a derived pool knob are hit
  the same way.

## What would close it

Restore the composition, so the derived 0 reaches the image again. Do NOT
fix it by stating `max_subscribers = 0` in the leaf: that hides the
derivation regression, which is the thing this leaf exists to witness.
Then show `case_19_xrce_rust_pubsub_e2e` green.
