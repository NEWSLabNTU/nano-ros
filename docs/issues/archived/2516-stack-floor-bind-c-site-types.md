---
id: 2516
title: "The bound stack walk sends every C indirect call to every function C can hold, because GCC objects carry no call-site types"
status: resolved
type: tech-debt
severity: low
area: [memory, tooling, rmw]
related: [issue-2402, issue-2400, issue-2517, phase-557]
found: 2026-11-14
---

## What

`scripts/nros-stack-floor.py --indirect bind` (issue 2402) follows every call
through a register. It narrows a Rust site from the codegen IR, first to the
vtable slot, then to the function type.

A site in C (zenoh-pico, the platform C, the board C) has no IR, because GCC
compiles it. So the walk sends it to every function C can hold: each function
whose address a C object takes, plus each Rust function whose address escapes
as a value. That is an upper bound, but a loose one.

## Measured

On `examples/mps2-an385-baremetal/rust/talker`, 126 C sites go to that set.
The static walk is 51,240 B, against a run that peaks between 31 and 32 KiB
(the MPU guard bisect, issue 2400 W5b).

The Rust half of the path matches the run frame for frame, down to
`ZenohPublisher::new`. All of the excess comes after that:

- zenoh-pico's generic containers call `elem_clear` / `elem_copy` /
  `elem_free` through pointers;
- each such site reaches every other C callback;
- so unrelated teardown, handshake and dispatch code chains into one path.

## Fix

Take C call-site types from clang. The proposal:

- Compile the same zenoh-pico sources with `clang -S -emit-llvm`, using the
  defines and includes the build used. Do this for analysis only; the shipped
  objects stay GCC's.
- Feed the result to `parse_ir_sites` like the Rust bitcode. A call site's
  type comes from the C source, so it does not depend on the compiler.

Two things need care:

- **Inlining.** GCC and clang inline differently, so a site can sit in a
  different function in each. Build the clang IR at `-O0`. When a GCC
  function has machine sites that its `-O0` IR lacks (inlined from a callee),
  give it the union of its callees' site types, or keep today's set for it.
- **Matching names.** A `static` function's name must match between the two
  objects. GCC suffixes clones (`.isra.0`, `.part.0`); `alias_gcc_clones`
  already folds those.

## Acceptance

The mps2 talker's walk drops toward its run and stays at or above it. The
nucleo-f767zi `[[observed]]` rows still hold (the walk covers each peak).

## Resolution (2026-11-14, phase-557 W7d)

`--compile-commands` (the cmake road passes the build's
`compile_commands.json`). Each C translation unit whose object the link map
names is compiled again by clang to bitcode, using:

- the build's defines and forced includes;
- its target flags;
- GCC's own ordered include search, from `cc <flags> -E -v`, with GCC's
  builtin header dir swapped for clang's.

The bitcode is typed-pointer: clang 14's opaque mode crashed on 6 Zephyr
TUs, and the walk's newer `llvm-dis` upgrades typed bitcode. It is cached by
clang version, command, source and object stamp.

How C sites bind now:

- **Matching.** Types are compared coarsely, by class per argument: pointer,
  <=32-bit integer, i64, float, double, aggregate.
- **C sites.** A C function's machine sites go to the functions C can hold
  whose type matches a site in its own `-O0` body, or in a same-TU function
  it reaches by direct calls (what GCC may have inlined). With no site in
  that closure, its branches are not calls.
- **Rust sites.** A typed C function joins a typed Rust site by coarse type.
- **Unsupported TUs.** A TU clang cannot compile leaves every function its
  GCC object defines bound as before. That is zpico.c: clang 14 refuses
  `__atomic_store_n` on `_Atomic(bool)*`.

Measuring this also closed two holes. Both now bind as C sites:

- a C function that Rust IR only declares kept no machine sites;
- inline-asm branches were not sites.

Measuring also found issue 2550: the walk is order-dependent under call
cycles.

Measured, as the deepest of 8 callee orders, with and without C types:

| image | without | with |
| --- | ---: | ---: |
| nucleo Rust action server | 85,208 | 71,352 |
| nucleo Rust talker | 81,960 | 69,776 |
| nucleo C talker | 140,252 | 104,840 |

The C talker still reads 9x its run: zpico.c is uncovered, and issue 2517
applies.

