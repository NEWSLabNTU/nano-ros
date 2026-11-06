# ESP32 — retired

<!-- type: how-to · level: 2 -->

nano-ros no longer supports the ESP32. Two paths existed, and both have been
removed: the ESP32-C3 bare-metal board over `esp-hal`, and the ESP-IDF
component with its `idf.py` build driver. This page says what each was, why it
went, and what an ESP32 port would need today.

## The ESP32-C3 bare-metal board (`esp-hal`), retired 2026-11-02

`esp32` was the ESP32-C3 under Espressif's QEMU fork, over `esp-hal`: the
`nros-board-esp32-qemu` board crate, the `nros-platform-esp32-qemu` platform
crate and the `examples/esp32-c3-baremetal/` projects. It had already been
dormant: nothing built or ran it.

nano-ros supports a way of running only where it keeps its real-time promise:
an executor parks until an arrival or its next deadline, and wakes for nothing
else. On a bare-metal board that needs a receive interrupt and a one-shot timer
behind the port's wake object, as `mps2-an385-baremetal` has. The ESP32-C3
board had neither, so its executor could only poll the network. nano-ros
refuses such a board rather than serve it by polling, so the board was retired
instead of ported.

Every name the board answered to (`esp32-c3-baremetal`, `esp32-qemu`,
`esp32-c3`, `esp32c3`, `esp32`) is refused with that reason wherever a board is
read: `system.toml`, `nros build`, `nros setup`, `nros::main!`. The `esp32`
framework name is refused the same way.

## The ESP-IDF component and `idf.py` driver, retired 2026-09

`integrations/nano-ros/` was an ESP-IDF component shim. Nothing built it, it
had no platform descriptor or `package.xml`, and it was never published to the
Espressif Component Registry, so it was removed rather than kept as an
untested promise.

`nros build` also had an `idf.py` driver for ESP32 boards whose package graph
crossed languages. With the component gone there was nothing for a project to
depend on, no ESP-IDF project for `idf.py` to find, and no way to carry a
resolved nano-ros setting into that build, so the driver was deleted. That
combination now **refuses** with those reasons instead of running a tool with
no project to build. C and C++ on an ESP32 have no build road.

## If you need an ESP32

The platform ABI and the C API are unchanged, and nothing forbids an
out-of-tree port. A port has to bring what every supported board has:

- for bare metal, the network driver's receive interrupt and a one-shot timer
  signalling the port's wake object, so the session reports arrivals instead
  of being polled;
- for ESP-IDF, a component, a way to carry nano-ros's resolved settings into
  the ESP-IDF build, and a build that something runs.

[Add a board](../extending/add-a-board.md) and [Add a platform](../extending/add-a-platform.md) describe what a port provides.
