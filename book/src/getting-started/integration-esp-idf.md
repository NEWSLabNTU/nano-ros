# ESP32 (ESP-IDF component) — retired

**The ESP-IDF integration has been removed.** This page is kept
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

## The ESP32 bare-metal board went too (2026-11-02)

The other ESP32 path, the ESP32-C3 QEMU **bare-metal** board over `esp-hal`,
was retired too: it could not keep the real-time wait promise. See [ESP32 (esp-hal) — retired](./esp32.md).

## The `idf.py` build driver went too (2026-09-28)

Removing the component left one piece standing: `nros build` still had an `idf.py` driver, chosen
for an ESP32 board whose package graph crossed languages, on the reasoning that
it would hand off to a project that was now necessarily yours. It was
deleted on 2026-09-28 because it could not have worked — with the
component shell gone there was nothing for your project to depend on, nothing
emitted an ESP-IDF project for `idf.py` to find, no mechanism carried a
resolved nano-ros setting into that build, and `nros setup` could not install
ESP-IDF in the first place.

That combination now **refuses**, naming the four reasons and
[the issue tracking a C/C++ ESP32 build road](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/1525-esp32-c-cpp-has-no-build-road.md),
instead of running a tool that had no project to build.

## If you need ESP-IDF

Nothing here forbids an out-of-tree ESP-IDF integration — the C API and the
platform ABI are unchanged, and `packages/platform/nros-platform-api/` still
describes what a port must provide. What is gone is an in-tree component that
nothing built and nobody could tell was unbuilt.

That issue lists what an in-tree road would have to bring back: a component, a
knob carrier for that build, an `nros setup` entry, and a lane that builds it.
That is the same bar `library.json` already sets for re-advertising the
`espidf` PlatformIO framework — *"re-add only alongside a real integration"*.
