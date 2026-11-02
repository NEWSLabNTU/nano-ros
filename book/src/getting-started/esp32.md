# ESP32 (esp-hal) — retired

**The ESP32-C3 bare-metal board has been removed.** This page is kept so the
links that point at it still resolve; the instructions it used to carry have
been deleted rather than left to be followed.

## What was here, and why it went

`esp32` was the ESP32-C3 under Espressif's QEMU fork, over `esp-hal`: the
`nros-board-esp32-qemu` board crate, the `nros-platform-esp32-qemu` platform
crate and the `examples/esp32-c3-baremetal/` leaves. It had already been
dormant: no lane built or ran it.

nano-ros supports a way of running only where it keeps its real-time promise:
an executor parks until an arrival or its next deadline, and wakes for nothing
else. On a bare-metal board that needs a receive interrupt and a one-shot timer
behind the port's wake object, as `mps2-an385-baremetal` has. The ESP32-C3
board had neither, so its executor could only poll the network. nano-ros
refuses such a board rather than serve it by polling, and the board was retired
instead of ported (2026-11-02).

## What happens if you name it

Every name the board answered to (`esp32-c3-baremetal`, `esp32-qemu`,
`esp32-c3`, `esp32c3`, `esp32`) is refused with that reason wherever a board is
read: `system.toml`, `nros build`, `nros setup`, `nros::main!`. The `esp32`
framework name is refused the same way.

## If you need an ESP32

Porting it back means giving the board what every bare-metal board needs: the
network driver's receive interrupt and a one-shot timer signalling the port's
wake object, so the session can report arrivals instead of being polled. The
platform ABI (`packages/platform/nros-platform-api/`) is unchanged. C or C++
on an ESP32 never had a build road.
