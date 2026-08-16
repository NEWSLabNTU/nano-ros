---
id: 638
title: The generated Zephyr entry TU has only a TARGET edge to the nros-cpp cargo
  build, so it races the header it includes and hits the `#error` stub
status: resolved
type: bug
area: build
related: [0090, 0282, 0326, 0088]
---

## Symptom

A Zephyr fixture row fails to compile its own generated entry:

```
FAILED: [code=1] CMakeFiles/app.dir/nros-entry/zephyr_entry_main.cpp.obj
In file included from …/nros-cpp/include/nros/config.hpp:18,
                 from …/nros-cpp/include/nros/action_client.hpp:17,
                 from …/nros-cpp/include/nros/component.hpp:37,
                 from …/build-c-listener-cyclonedds/nros-entry/zephyr_entry_main.cpp:21:
…/nros-cpp/include/nros/nros_cpp_config_generated.h:59:2: error: #error
  "nros_cpp_config_generated.h must be supplied per-build by the build system"
```

The stub is reached because the real per-build header — written by the nros-cpp
cargo build into `<build>/nros-rust/nros-cpp-generated/nros/` — did not exist
yet when this object was compiled.

Because the whole family is one make driver, this aborts every remaining Zephyr
row behind it.

## It is a race, and the evidence is that two identical rows disagree

In a single `just zephyr build-fixtures`:

| row | entry TU | result |
| --- | --- | --- |
| `build-c-talker-cyclonedds` | `nros-entry/zephyr_entry_main.cpp` | compiled AFTER the cargo build — linked, `zephyr.exe` produced |
| `build-c-listener-cyclonedds` | the same generated TU | compiled BEFORE it — `#error` stub |

Same platform, same language (`lang = c`), same RMW, same configure run
(both `CMakeCache.txt` stamped 13:30), both with `nros_cpp_cargo_build` in
their ninja and both with the `nros-cpp-generated/` directory present. The
header file itself was on disk for the talker and absent for the listener.

The only difference is scheduling. That is the signature of a missing edge, not
of a missing feature — and it means the talker has been passing by luck.

## Root cause, which the code already states while exempting itself

`cmake/NanoRosNodeRegister.cmake` gives component sources a FILE-level
`OBJECT_DEPENDS` on the generated headers, and explains exactly why a target
dependency is not enough:

> `_nros_node_register_config_header_deps` (above) orders the target but ninja
> can still start the object compile early (issue 0090); a file-level
> `OBJECT_DEPENDS` forces each TU to wait for the generated headers.

The very next sentence exempts this case:

> (A C node / the single-node carrier compiles into `app`, which already
> depends on the cargo build, so neither hits this.)

But `app` is a TARGET dependency — the kind the preceding sentence has just
finished describing as insufficient. The generated entry is attached with
`target_sources(app PRIVATE "${_zephyr_entry_src}")` and never receives a
file-level edge, so ninja is free to start its object compile before the cargo
build has written the header. Which it does, sometimes.

This is the same class as issues 0088/0090/0282/0326 — a generated header
reached through an ordering that does not actually order the compile.

## Fix

Give `_zephyr_entry_src` the same file-level `OBJECT_DEPENDS` the component
sources get, `APPEND`ed rather than set — the Zephyr interface-codegen module
stamps `OBJECT_DEPENDS` on sources too, and a plain
`set_source_files_properties` from either side clobbers the other, which is the
warning the sibling site already carries.

## Fixed 2026-08-16

`_zephyr_entry_src` now gets the file-level `OBJECT_DEPENDS`, `APPEND`ed for the
reason the sibling site documents.

Verified structurally rather than by one green build, because a race that
happens to go the right way proves nothing. The generated ninja for the row that
failed now carries the edge as an IMPLICIT dependency — `|`, which orders and
rebuilds, not `||`, which only orders (the distinction issue 0475 turns on):

```
build CMakeFiles/app.dir/nros-entry/zephyr_entry_main.cpp.obj: CXX_COMPILER__app_ \
    …/nros-entry/zephyr_entry_main.cpp \
  | nros-rust/nros-cpp-generated/nros/nros_cpp_config_generated.h \
    nros-rust/nros-c-generated/nros/nros_config_generated.h
```

and the row builds to `zephyr.exe` from a wiped build dir.

The talker row is unchanged in behaviour and was never the bug — it was the
control that made this diagnosable, by succeeding where its twin failed.

## Why it surfaced now

The row was never reached before: `build-c-listener-cyclonedds` died earlier at
configure time on issue 0633's `idlc` resolution. Fixing that let the row get
as far as compiling, where this was waiting. Nothing here is new — the race has
been latent for as long as the exemption has.
