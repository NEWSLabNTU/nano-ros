---
id: 283
title: "ROS 2 liveliness is disabled wholesale on FreeRTOS (phase-127.B workaround) — MCU lanes are invisible to `ros2 node list`; re-enable per-platform"
status: resolved
type: enhancement
severity: medium
area: zpico
related: [issue-0269, issue-0274]
---

## Finding (autoware_sentinel phase-14.5 embedded entries, 2026-07-26)

`nros-rmw-zenoh`'s session shim gates EVERY liveliness declaration —
the per-node NN token and every entity token — behind a compile-time
platform check:

```rust
// packages/rmw/zenoh/nros-rmw-zenoh/src/shim/session.rs
fn should_declare_liveliness(&self) -> bool {
    // FreeRTOS QEMU/slirp peer-to-peer fixtures do not need ROS 2
    // discovery tokens for data routing, and current zenoh-pico FreeRTOS
    // liveliness declaration can block once another peer is present.
    !cfg!(feature = "platform-freertos")
}
```

Introduced by `6866903ab` ("phase-127.B: fix FreeRTOS zenoh liveliness",
2026-05-16) as a workaround for a *blocking* declaration bug — a real
hang at the time. The consequence today: a FreeRTOS image can publish
and subscribe perfectly, but **no ROS 2 tool can see it** — `ros2 node
list` / `topic list` / `node info` are all empty against a fully working
firmware, because rmw_zenoh_cpp derives the entire graph from those
tokens.

The sentinel's 14.5 MCU entries make this concrete: the NuttX lane
(liveliness on) shows all 10 nodes from the host through QEMU SLIRP →
zenohd, while the byte-equivalent FreeRTOS lane shows none. We initially
chased that asymmetry as a sentinel bug; it is this cfg.

## Why it should come back

1. **`platform-freertos` is not "QEMU fixtures".** The cfg is a
   platform feature, so it also silences liveliness on real FreeRTOS
   hardware (the sentinel's Cortex-M safety-island target, NVIDIA's
   Orin SPE via `platform-freertos`, every downstream FreeRTOS board) —
   far beyond the slirp fixtures the comment describes.
2. **Graph visibility is an operational requirement,** not a nicety: it
   is how integrators confirm a safety MCU joined the system, and how
   `ros2 topic hz` / `node info` diagnose it in the field.
3. **The blocking bug may already be fixed.** The known FreeRTOS
   declare-path defects have since been addressed (issue 0269's slot
   exhaustion; the `_z_send_tcp` write_all drain loop landed with it).
   The workaround has not been re-tested against the current stack.

## Resolution (2026-07-26)

**Re-tested at HEAD: the blocking bug is GONE.** With
`should_declare_liveliness()` returning true on FreeRTOS, the sentinel's
10-node MPS2 image registers every entity, enters its spin loop and
keeps publishing — no hang, no error, with a zenohd peer present for the
whole run (the original 127.B trigger).

Landed here:
- The platform cfg is replaced by an explicit **`no-liveliness` cargo
  feature** on `nros-rmw-zenoh`. Default = liveliness ON for every
  platform, including FreeRTOS; fixtures that want the wire quiet opt
  out deliberately.
- `zpico_declare_liveliness` now logs a failed
  `z_liveliness_declare_token` (it was a silent graph outage; the
  publisher/subscriber declare paths already logged theirs).

**Residual CLOSED — it was the measurement, not the firmware.** The
"declares fine but invisible" reading came from a broken host probe:
the sentinel's `rmw_zenoh_cpp` overlay had been wiped (it symlinked into
a sibling checkout's build tree), so every `ros2` call was silently
answering from a stale/absent RMW, and the router binary the probe used
had vanished with it. With the overlay rebuilt in-repo and the router
taken from the overlay itself (`rmw_zenohd`), the FreeRTOS lane shows
its FULL graph from the host:

```
$ scripts/probe_mcu_graph.sh freertos      # in autoware_sentinel
== nodes:      /adapi/default_adapi … /system/mrm_pull_over_manager   (10)
== topics:     42
== typed echo of /control/command/control_cmd: live data
```

NuttX re-probed identically (10 nodes / 42 topics) — no regression from
the cfg removal. Liveliness on FreeRTOS works at HEAD; the phase-127.B
workaround was obsolete, and nothing else was hiding behind it.

## Ask (original)

- Re-test liveliness declaration on FreeRTOS at HEAD: single peer, then
  with a second peer present (the original blocking trigger).
- If it no longer blocks, delete the cfg and let every platform declare.
- If it still blocks, narrow the escape hatch from "all FreeRTOS
  forever" to something a product can opt out of deliberately — e.g. a
  `no-liveliness` cargo feature or a `Config` field — so hardware images
  are visible by default and only the affected fixtures opt out.
- Same question for the other RTOS lanes: audit whether ThreadX /
  bare-metal carry equivalent silent gates.

## Repro (sentinel side, for the re-test)

autoware_sentinel branch `phase-14`:

```sh
just build-sentinel-freertos   # or: cd src/freertos_entry && cargo build --release
zenohd --listen tcp/0.0.0.0:7447 &
qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic \
  -semihosting-config enable=on,target=native \
  -kernel src/freertos_entry/target/thumbv7m-none-eabi/release/freertos_entry
# guest: registers 10 nodes, spins, publishes
# host:  ros2 node list -> empty
# NuttX control (liveliness on): src/nuttx_entry -> all 10 nodes listed
```
