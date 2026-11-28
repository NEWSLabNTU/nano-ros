---
id: 3025
title: "Component instance caps are hand-set (default 2), never derived from how many nodes of a class the image runs"
status: open
type: tech-debt
severity: low
area: [sizing, components, c-api, codegen]
related: [issue-1033, issue-1130, rfc-0100]
found: 2026-11-28
---

## What

Two pools bound how many nodes of ONE component class an image may run:

- Rust: `NROS_RUNTIME_MAX_CLASS_INSTANCES`. This sizes each
  `nros::node!` class's `ComponentSlotStorage` (`nros-macros`) and is read
  by `packages/api/nros/build.rs` from env, then a platform/board rung, then
  a builtin 2.
- C: `RCLC_C_COMPONENT_MAX_INSTANCES`. This sizes `RCLC_C_COMPONENT`'s
  per-package instance table (`rclc/component.h`). It is a header default of
  2, raised by a `-D` on the component's compile. It arrives with the fix for
  the C components that used to share one instance.

Neither is derived. The model knows exactly how many launch nodes name each
package (`pkg`/`exec`), but an image running three nodes of one class still
needs a hand-set knob or `-D`. Without one, it gets a boot-time refusal: a
loud one, naming the package and the knob. That is correct but avoidable.
Every other pool here is a DEFAULT derived from the image's declarations
(issue 1033's rule), with the builtin kept only for an image that states
nothing.

## Shape of the fix

- **Count.** The entity inventory counts nodes per `(pkg, class)` from the
  resolved model, the same set the generated entry instantiates. That gives
  the largest count for any Rust class and a per-package count for C.
  Without a model, nothing is stated and the builtin 2 stands.
- **Rust.** Add a derived rung for `RUNTIME_MAX_CLASS_INSTANCES` (the max
  over classes), carried the way the other runtime knobs are on each road:
  the cargo leaf's generated config, cmake's `NROS_DERIVED_*` and the west
  resolver. A stated knob still wins.
- **C.** The cap is a per-PACKAGE compile definition beside
  `NROS_PKG_NAME`, which is set at three sites today
  (`cmake/NanoRosNodeRegister.cmake` ×2, `cmake/NanoRosVerbs.cmake`). It
  has to reach every road a C component builds on: cmake, west, and the
  NuttX board's recompile of `SOURCES`. That makes it a carrier question
  (canonical-build-path.md), not a formula: one source of the count, and a
  check that each road carries it.
- **Test.** A three-instance C image and a three-instance Rust image boot
  with no `-D` and no knob. A stated value below the count still refuses at
  boot, naming the package.

## Why it is not in the correctness fix

The correctness fix (one instance per node, refusal past the cap) is
self-contained and boots every in-tree image, none of which runs more than
two nodes of one class. Derivation touches the inventory, the knob
registry's rung for one knob, and per-package delivery on four build roads.
