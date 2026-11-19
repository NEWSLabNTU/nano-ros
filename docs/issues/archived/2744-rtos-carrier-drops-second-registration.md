---
id: 2744
title: "A second component registered in an image package was built into nothing: the carrier image shipped with the first component only"
status: resolved
type: bug
severity: high
area: [cmake, rtos, owned-entry]
related: [rfc-0071, issue-2737]
found: 2026-11-20
resolved: 2026-11-20
resolved_in: "the change that archives this issue"
---

## Measured

Measured on `examples/rv-virt-threadx/c/typed-carrier`, scratch copy, with a
second `nros_components_register_node(c_typed_carrier_lib_b … EXECUTABLE
c_typed_carrier_b DEPLOY threadx)` added. `nros build` succeeded and produced
ONE firmware, `pkg/riscv64_threadx_c_typed_carrier/riscv64_threadx_c_typed_carrier`.
Its symbols name only the first component. `libc_typed_carrier_lib_b.a` was
built and linked into nothing.

## Why

Each single-node carrier (NuttX, ThreadX, FreeRTOS, native typed, Zephyr) is
created behind a `NOT TARGET ${PROJECT_NAME}` guard (Zephyr:
`${PROJECT_NAME}_nros_zephyr_entry`). The first registration creates it.
Every later one fails the guard and builds only its component library, with
no message. The `EXECUTABLE` a registration states names nothing on these
roads: the carrier is named after the package.

## Resolution

`_nros_carrier_claim` (cmake/NanoRosNodeRegister.cmake) is the one rule all
five carrier roads go through.

- A registration that names a DEPLOY (the package IS an image) and would
  build a carrier the package already has is a configure error, naming both
  registrations.
- A registration with no DEPLOY is a workspace member's component, which a
  generated entry composes from its library. It keeps the old behaviour: no
  second carrier, nothing lost.
- A carrier target the package defines itself (`add_executable(<project>)`)
  still turns the carrier off.

Verified:
- the scratch copy above now fails at configure, naming `c_typed_carrier`
  and `c_typed_carrier_b`;
- the unmodified `threadx-riscv64-c-zenoh-typed-carrier` row builds;
- `workspace-c-native` (TYPED members, one registration each) builds.

Tests: `cmake_node_register_misuse`:
- `a_second_carrier_registration_is_refused`;
- `a_second_member_registration_without_deploy_is_not_refused`, the
  negative control.

Mutation: dropping the DEPLOY condition from the refusal fails the first test.

Building one carrier image per `EXECUTABLE`, so that a package can hold
several programs, is the RFC-0071 D19 amendment ruled on 2026-11-20 and is
tracked by its phase doc. This issue closes the silent drop, which ships a
wrong image in the meantime.
