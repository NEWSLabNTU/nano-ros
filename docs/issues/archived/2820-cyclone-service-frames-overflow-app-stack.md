---
id: 2820
title: "Four Cyclone service functions each reserve a 64 KiB stack frame, so a Cyclone image serving services overflows a 64 KiB FreeRTOS app task"
status: resolved
type: bug
severity: high
area: [rmw, cyclonedds, freertos, memory]
related: [2818, 2819, 1624]
found: 2026-11-22
resolved: 2026-11-22
resolved_in: "the change that resolved it"
---

## What happened

The tier-2 run on main (2026-11-21) failed
`freertos_qemu::an536_cyclonedds_cpp_entry_delivers_within_the_dds_heap_default`:

```
I heard: [Hello World: 1]
*** STACK OVERFLOW: app ***
```

The same image (`workspace-cpp-mps3-an536-freertos`, talker + listener over
Cyclone on qemu `mps3-an536`) was measured on 2026-10-02 at a 65,536-byte app
stack and delivered (issue 1624's `heap peak 447944`). So this was a
regression, not a stale default.

## Measured

- Raising only the app stack to 128 KiB: 76 deliveries, `app task stack peak
  77916 of 131072 bytes`.
- A prologue scan of the image (`arm-none-eabi-objdump -d`, every
  `sub sp, #N` with N ≥ 4096) found four functions reserving 65,536 bytes each:
  `service_take_request_len`, `service_send_response`,
  `service_send_request_raw` and `service_take_response_raw_len`, in
  `packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/service.cpp`. Each held a
  `uint8_t wire[kWireScratch]` local (`kWireScratch = 65536`).
- The frame is reserved on entry, so any call overflows a 64 KiB stack, even
  one that takes nothing. `Executor::pass` (a 9.8 KiB frame) reaches these
  functions on every spin of an image that serves a service.
- The buffers are old (2026-07-31). What changed is that every image now
  serves the stock node interfaces by default (`6229085857`, 2026-11-14), so
  the parameter services are polled on every spin of this entry.
- Experiment: `kWireScratch = 16384` at the stock 64 KiB stack gave 75
  deliveries, `app task stack peak 28772 of 65536`, and `heap peak 639160`,
  under the 655,360-byte DDS heap default. The heap outside the stack was
  unchanged (573,624 vs 573,680 bytes), so the heap default stands.

## Fix

No wire-sized buffer on the stack:

- The takes copy out of the serdata in three pieces (`ddsi_serdata_to_ser`):
  the encap into the caller's span, the 16-byte header into a local, and the
  user fields after the encap. The new helper is `take_service_wire`.
- The reply write passes the caller's span and the header to `dds_write` as
  one `NrosCdrBlob` with an inserted segment (`nros_sertype.hpp`).
  `from_sample` assembles the wire inside the serdata it already allocates.
- The request send builds straight into the client's `pending_request`
  member, so the stack copy it used to make is gone.

A side effect: a service payload larger than 64 KiB is no longer refused on
the take side, because the bound is now the caller's buffer. The client's
send is still bounded by `pending_request` (issue 2819).

The stack walk did not flag these frames: it refused the image as
UNMEASURED and enforced nothing. That class is issue 2818.
