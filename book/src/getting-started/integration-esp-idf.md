# ESP32 (ESP-IDF component) — retired

**The ESP-IDF integration has been removed (phase-468 W2).** This page is kept
so the links that point at it still resolve; the instructions it used to carry
have been deleted rather than left to be followed.

## What was here, and why it went

`integrations/nano-ros/` was an ESP-IDF component shim, driven by a
`just`-module of its own. Measured on 2026-09-25, before removing it:

* no workflow under `.github/` built it;
* `examples/fixtures.toml` had no row naming it;
* it was the only one of six pure-C platform ports with neither an
  `nros-platform.toml` descriptor nor a `package.xml`;
* it had never been published to the Espressif Component Registry — the
  release doc records the publication step as not performed.

The book had nonetheless advertised it as **Ready** in two support tables, on
the strength of a nightly lane that did not exist. That claim was the reason to
look, and looking is what retired it.

## What still works: ESP32 bare-metal

**The ESP32 support that has a booting CI lane is unaffected**, and it is a
different thing one hyphen away. `esp32` is the ESP32-C3 QEMU **bare-metal**
path over `esp-hal`, with its own board crate, fixtures and nightly lane:

* [ESP32 (esp-hal)](./esp32.md)
* `examples/esp32-c3-baremetal/`

If you are targeting an ESP32-C3, that is the page you want.

## If you need ESP-IDF

Nothing here forbids an out-of-tree ESP-IDF integration — the C API and the
platform ABI are unchanged, and `packages/platform/nros-platform-api/` still
describes what a port must provide. What is gone is an in-tree component that
nothing built and nobody could tell was unbuilt.
