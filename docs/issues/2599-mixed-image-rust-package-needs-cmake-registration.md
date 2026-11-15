---
id: 2599
title: "A Rust package in a mixed image needs a hand-written CMake registration that restates its [[component]] row"
status: open
type: design-question
area: [cli, cmake, build]
severity: medium
found: 2026-11-15
related: [phase-478]
---

GitHub: NEWSLabNTU/nano-ros#3857

**Home: [phase-478](../roadmap/phase-478-island-developer-experience-findings.md), I13.**
Re-reported by the Autoware Safety Island on 0.6.0-nros10.

## What happens

An image that runs C or C++ nodes builds through CMake. A Rust node package
in that image is invisible to the configure unless it carries:

- a `CMakeLists.txt` calling
  `nano_ros_node_register(NAME … CLASS … LANGUAGE RUST SOURCES Cargo.toml)`;
- `<build_type>nros_cmake</build_type>` in its `package.xml`.

Without them the build stops with `has no matching component in
nros-metadata.json`. Issue 2565 made `nros new … --to <bringup>` write both
and documented them (book: "Mix languages"). That removes the surprise, but
not the requirement: delete the shim and the build still fails.

Every fact the shim states is already stated once in the bringup:
`[[component]] pkg` and `class` name the package and the class, `name` the
instance, and the package's `Cargo.toml` is the source. The shim is a second
copy, kept in step by hand.

## The question

Should a Rust package in a mixed image need no registration of its own?

## Options

**A. Derive the registration.** The generated CMake root (`builder/cmake_root.rs`)
emits `nano_ros_node_register(… LANGUAGE RUST …)` itself for each
`[[component]]` row whose package is a cargo crate with no `CMakeLists.txt`.
The class and package come from the row, and the source is the package's
`Cargo.toml`. A package that ships its own `CMakeLists.txt` keeps it, as
today.
- For: one statement per fact. The same Rust package builds unchanged in a
  pure-Rust image and in a mixed one. `nros new` stops writing a file, and the
  "Mix languages" step about it goes.
- Against: the root learns to register one kind of package it does not own.
  The metadata probe and the native and Zephyr roots must all derive it the
  same way, so the rule needs one home that all three call.

**B. Keep the shim as the documented answer.** `nros new` writes it, the book
says so, and a missing shim is refused with a message naming the two lines to
add.
- For: no new derivation. A registration is explicit wherever it exists.
- Against: the shim restates the bringup, and the island's report is exactly
  the drift a second copy invites.

**Recommendation: A.** The row already states everything the shim does, so
the shim is a copy, and the repo's rule is to derive a copy rather than keep
it by hand. B is a smaller change but leaves the requirement in place.

## Workaround

Keep the shim that `nros new … --to <bringup>` writes, or write it by hand as
the "Mix languages" page shows.

## Ruling (2026-11-15)

User ruling, relayed by the phase-478 manager: **option A**. The generated
CMake root derives `nano_ros_node_register(... LANGUAGE RUST ...)` for a
cargo-only component package, and a package that ships its own
`CMakeLists.txt` keeps it. `nros new` and the "Mix languages" page stop
writing and documenting the shim for the derived case.
