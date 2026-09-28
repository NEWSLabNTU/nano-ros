---
id: 1523
title: "`NROS_COMPONENT_LANG` is written lowercase by one verb and uppercase by
  another, and all three readers test `STREQUAL \"C\"` — so a
  `nano_ros_auto_add_library` C component never takes a C branch"
status: resolved
resolved_in: 2026-09-28
type: bug
area: [cmake, build]
severity: low
related: [0425, 1467, 1062]
---

## What this is

Found while collapsing the three extension→language copies (phase-469 S3). It
is a SEPARATE defect from that one — the same fact with two SPELLINGS rather
than two DERIVATIONS — and it is the CLAUDE.md pitfall "case-normalize enum-ish
cmake args" live in three comparisons.

`_nros_infer_lang()` answers in the canonical lowercase (`c` / `cpp`, which is
`nros_lang::Language::as_str` and the on-disk serde contract).
`nano_ros_node_register()` keeps an UPPERCASE vocabulary internally (`C` / `CPP`
/ `RUST`, ~30 comparisons). Both write the same target property, and every
reader tests the uppercase form:

| site | what it does |
| --- | --- |
| `cmake/NanoRosVerbs.cmake:397` | `nano_ros_auto_add_library` writes `NROS_COMPONENT_LANG "${_lang}"` — LOWERCASE |
| `cmake/NanoRosNodeRegister.cmake:747` | `nano_ros_node_register` writes it — UPPERCASE |
| `cmake/NanoRosVerbs.cmake:480` | `nros_components_register_node` reads it, `if(_ncr_lang STREQUAL "C")` |

`STREQUAL` is case-sensitive, so for every target created by
`nano_ros_auto_add_library` that reader is permanently FALSE, and two more
comparisons inside `nano_ros_auto_add_library` itself read the same lowercase
value against `"C"`:

* `cmake/NanoRosVerbs.cmake:375` — `if(_lang STREQUAL "C")` →
  `set_target_properties(… LINKER_LANGUAGE C)` is **never** executed, so a pure-C
  component library is linked with the C++ driver.
* `cmake/NanoRosVerbs.cmake:388` — `if(NOT _lang STREQUAL "C")` → **always**
  taken, so a pure-C component library links `NanoRos::NanoRosCpp` through the
  branch whose comment says it is for C++.

## Why nothing broke

The umbrella outcome is accidentally the one issue 0425 wants. That issue's
conclusion is "prefer the C++ umbrella whenever it exists, TYPED or not,
because it BUNDLES nros-c", and the always-taken `NOT … STREQUAL "C"` branch
links exactly that. Since issue 1467 the `PUBLIC` pick is wrapped in a
`$<TARGET_PROPERTY:NROS_CARRIES_RUST_RUNTIME>` genex, so a Rust-rooted consumer
still drops it. So the two wrong branches cancel into the right link line, and
the reader at `:480` is dead code for one of its two producers.

What is NOT compensated is `LINKER_LANGUAGE`: an all-`.c` component library
built through `nano_ros_auto_add_library` (e.g.
`examples/workspaces/realtime-c/src/ctrl_pkg`, `nano_ros_auto_add_library(ctrl_lib
STATIC src/Ctrl.c)`) links with the C++ driver.

## Why it is filed rather than fixed

Correcting the comparisons CHANGES LINK LINES for every component library built
through `nano_ros_auto_add_library`, and the acceptance for that is a build of
the C and mixed workspaces on more than one platform — not the configure-level
acceptance phase-469 S3 earned. Landing it inside an unrelated commit would also
make a link regression indistinguishable from the language-inference change.

## What the fix looks like

One vocabulary for the property, not a `TOUPPER` at each reader. The canonical
spelling is the lowercase one (`Language::as_str`, the serde contract, what
`nano_ros_add_node` already forwards as `LANGUAGE ${_lang}`), so the direction
is to lower-case what `nano_ros_node_register` writes and move its ~30 internal
uppercase comparisons — or to keep the uppercase property and upper-case at
`:397`, which is smaller and leaves two vocabularies in the tree.

Either way the two comparisons at `:375` / `:388` are wrong today and the fix
must say which link line each one is expected to produce afterwards, measured.

A gate is worth considering with it: the class is "enum-ish cmake string
compared case-sensitively", already in CLAUDE.md's pitfall index with no gate
behind it.

## Resolution — 2026-09-28

Every site and both consequences were re-measured against `origin/main` before
anything changed. The two writers, the three readers and the dead reader are
exactly as filed. **One of the two stated consequences did not reproduce**, and
saying which is the point of this section.

### Where it was normalised, and why there

`nano_ros_node_register` already computes `_nrc_lang_lc` — it is what the
`"lang"` field of the metadata row beside the property carries — so the property
write became `_nrc_lang_lc`, and the three readers became lowercase. That is the
BOUNDARY at which the value leaves the file's uppercase vocabulary, the mirror
of the `TOUPPER` phase-469 S3 put at the boundary where the query's answer
enters it. The ~30 internal comparisons did not move: what matters is that
nothing uppercase LEAVES the file, and moving them would have been a large diff
whose defects would be indistinguishable from this one's.

Two things landed with it, both because the class is "a value with two
spellings and nothing comparing them":

* **The write has ONE spelling.** `cmake/NanoRosComponentLang.cmake`'s
  `_nros_set_component_lang(<target> <lang>)` is the only site that writes the
  property, and it REFUSES a value outside `c` / `cpp` / `rust` with a message
  naming the target and the conversion. All three writes go through it,
  including the conventional-name wrapper's mirror of an already-validated
  value — that wrapper is precisely where a value from a producer this file
  does not control would enter.
* **`check-component-lang-vocabulary`** (`just check component-lang-vocabulary`,
  fast lane, buildless, self-testing on the normal path, 8 negative controls).
  Rule 1: no site outside the owner writes the property. Rule 2: every
  `STREQUAL` against a variable bound by `get_target_property(… NROS_COMPONENT_LANG)`
  names a value from the canonical set, and every `STREQUAL` against a variable
  bound by `_nros_infer_lang()` / `nros_language_of_sources()` names a LOWERCASE
  literal — two strengths, because `nano_ros_add_node` legitimately compares the
  accepted alias `cxx` against the inference's answer in the same variable, and
  a membership rule there would be a false report. Vacuity guard: fewer than 1
  write or fewer than 3 comparisons is a FAILURE.

  Negative control on the real tree, not a fixture: re-introducing the uppercase
  property write **and** `STREQUAL "C"` at `:480` gives both violations by file
  and line; re-introducing `STREQUAL "C"` at `:375` / `:388` gives those two.
  Green tree: `OK (95 cmake file(s); 1 NROS_COMPONENT_LANG write(s), all in
  cmake/NanoRosComponentLang.cmake; 9 canonical language comparison(s), all
  lowercase)`.

### `LINKER_LANGUAGE` — the branch is live, and it changes nothing measurable

Measured with a temporary `message(STATUS)` in each of the three branches,
reconfiguring `examples/workspaces/c`:

```
1523probe: auto_add_library(talker_lib) lang='c'
1523probe: auto_add_library(talker_lib) SET LINKER_LANGUAGE C      <- never before
1523probe: register(talker_lib) NROS_COMPONENT_LANG='c'
1523probe: register(talker_lib) REVIVED C branch FIRES; NanoRosCpp target? YES
```

— for all six of that workspace's component libraries, and no
`auto_add_library … LINKS umbrella (cpp branch)` line, which fired for all six
before.

**The issue's claim that the library "is linked with the C++ driver" does not
reproduce.** CMake derives a static library's linker language from its own
sources, and every one of these has only `.c` files, so it already answered C:
`build.make` reads `Linking C static library libtalker_lib.a` **before and
after**, on native and on threadx-linux, and the archive rule is
`/usr/bin/ar qc` + `ranlib` in both — identical for C and CXX anyway. The
umbrella the library used to link PUBLIC did not push it to CXX either (checked
by measuring the before state with that link present). So what the fix buys here
is that the property now STATES what CMake had been inferring; the failure the
issue predicted needs a component library CMake cannot infer a language for, and
the tree has none.

### Link lines, before and after

Snapshot = every `link.txt` in the build tree plus every `Linking <LANG> …`
rule line from every `build.make`. Three build trees, each configured from
`origin/main` and again with the fix, diffed:

| build tree | platform | diff |
| --- | --- | --- |
| `examples/workspaces/c/build/posix-zenoh-native/cmake` (112 lines) | linux | **empty** |
| `examples/workspaces/mixed/build/posix-zenoh-native/cmake` (104 lines) | linux | **empty** |
| `examples/workspaces/mixed/build/threadx-linux-zenoh/cmake` (74 lines) | threadx-linux | **empty** |
| `examples/workspaces/mixed/build/freertos-zenoh-mps2-an385-freertos/cmake` (79 lines) | freertos (arm-none-eabi 13.2, cross) | **empty** |

The two representative libraries the issue asks about, after the fix (both
unchanged from before):

```
# C component — examples/workspaces/c, talker_pkg/talker_lib
/usr/bin/ar qc libtalker_lib.a CMakeFiles/talker_lib.dir/src/Talker.c.o
/usr/bin/ranlib libtalker_lib.a
                                      ("Linking C static library libtalker_lib.a")

# C++ component — examples/workspaces/mixed, cpp_listener_pkg/listener_lib
/usr/bin/ar qc liblistener_lib.a CMakeFiles/listener_lib.dir/src/Listener.cpp.o
/usr/bin/ranlib liblistener_lib.a
                                    ("Linking CXX static library liblistener_lib.a")
```

and the umbrella outcome on the consuming binary, also unchanged:

```
# examples/workspaces/c, native_entry — the C umbrella ladder's top rung
/usr/bin/cc … pkg/talker_pkg/libtalker_lib.a … \
  nano_ros/packages/api/nros-cpp/libnros_cpp.a -lgcc_s … 

# examples/workspaces/mixed, native_entry — NanoRosCpp retargeted onto the
# workspace runtime crate, so the same target under a different file name
/usr/bin/c++ … pkg/c_talker_pkg/libtalker_lib.a … libnros_ws_runtime.a -lgcc_s …
```

The identity is not luck and it is worth stating, because "correct branches must
still produce the line the wrong ones did" was the acceptance:

1. the umbrella moved from `nano_ros_auto_add_library`'s
   `CANDIDATES NanoRos::NanoRosCpp` to `nros_components_register_node`'s default
   ladder `NanoRos::NanoRosCpp NanoRos::NanoRos`. `NanoRos::NanoRosCpp` exists in
   every one of these configures, so both ladders resolve to the same target;
2. both call sites are `nros_link_runtime_umbrella(… PUBLIC)`, so both wrap the
   pick in the same issue-1467 genex; and
3. the entry executable links the umbrella PRIVATE by literal name anyway
   (`cmake/NanoRosEntry.cmake`), so the component's propagated copy was always a
   duplicate of a link the binary makes for itself.

Builds, all green after the fix: `workspace-c-native`, `workspace-cpp-native`,
`workspace-mixed-native` (linux), `workspace-mixed-threadx-linux`
(threadx-linux) and `workspace-mixed-freertos` (a genuine thumbv7m-none-eabi
cross, pinned arm-none-eabi 13.2) — three platforms, four build trees, one of
them cross-compiled.

NOT built, and named rather than glossed: **NuttX** (the platform the issue's
own `examples/workspaces/realtime-c/src/ctrl_pkg` example builds on), **Zephyr**
and **esp32**. NuttX and esp32 need source builds this host cannot afford
(`nros setup --source {cyclonedds-src,px4-rs,nuttx-libc}` plus the 406 MB
PX4-Autopilot submodule), and no Zephyr west workspace resolves in this
worktree. `examples/workspaces/c` carries six all-`.c`
`nano_ros_auto_add_library` targets of exactly `ctrl_pkg`'s shape, and the
FreeRTOS tree carries four more on a cross target, so the shape is covered even
where the platform is not. **Tier 2 (`just ci matrix`) was NOT run** for the
same reason — its preconditions are those three source builds, that submodule
and a `build-test-fixtures lane=tier2` — on a host at 98 % disk. That is the gap
a reviewer should weigh: the surface is every component library's link decision,
and it was measured identical on three of six platforms.

### The revived reader, and whether it is right

`nros_components_register_node`'s `STREQUAL "c"` arm now fires for the producer
it never fired for. It calls `nros_link_runtime_umbrella(${target} PUBLIC)` with
the DEFAULT candidate ladder, which on every configure measured picks
`NanoRos::NanoRosCpp` — the same target the branch it replaces picked, hence the
identical line.

It is correct, and it is strictly better than what it replaces in one case: a
configure that has only `NanoRos::NanoRos`. The old
`nano_ros_auto_add_library` branch passed a one-entry `CANDIDATES
NanoRos::NanoRosCpp`, which in that configure resolves to nothing and links no
umbrella at all; the ladder falls back to `NanoRos::NanoRos`, which is what
issue 0425 says a pure-C workspace should get. That rung is not exercised by
anything in this tree — and the comment beside it, which claimed "a pure-C
workspace instantiates no `NanoRosCpp` target and is unaffected", was FALSE:
`examples/workspaces/c` is pure C in its sources and DOES instantiate
`NanoRosCpp`, because every leaf `add_subdirectory()`s the nano-ros root, which
is issue 1467's own observation one file over. That comment is corrected, as is
a second stale one at both sites claiming the C pick turns on `TYPED` (issue
0425 made it unconditional).

### The 1467 genex still works on the call shape the C branch now uses

Negative control — a throwaway standalone configure (it includes
`cmake/NanoRosRuntimeUmbrella.cmake`, links a component library's umbrella with
`nros_link_runtime_umbrella(<lib> PUBLIC)` — the exact call the C branch now
makes — and gives that library two consumers, one of which calls
`nros_declare_rust_runtime_carrier`):

```
plain_consumer:        cc … -o plain_consumer  libcomp_lib.a libfake_umbrella.a
rust_rooted_consumer:  cc … -o rust_rooted_consumer  libcomp_lib.a
```

The carrier drops the umbrella; the plain consumer keeps it. The only in-tree
producer of `NROS_CARRIES_RUST_RUNTIME` is
`cmake/board/nano-ros-board-rv-virt-threadx.cmake`, and no `threadx-riscv64`
leaf contains a `nano_ros_auto_add_library` target, so no real image's umbrella
decision moved.

### Sweep

```
grep -rn 'STREQUAL "\(C\|CPP\|CXX\|RUST\|RS\|c\|cpp\|cxx\|rust\|rs\)"' cmake/
```

18 sites outside `NanoRosNodeRegister.cmake`'s own uppercase vocabulary. Every
other one was already normalised at its boundary: `nros_generate_interfaces()`
`TOUPPER`s `_ARG_LANGUAGE` at entry and its four `STREQUAL "CPP"` readers plus
`_nros_predict_generated_outputs`'s two (its one caller) sit behind that;
`nano_ros_add_node` `TOLOWER`s caller input; `nano_ros_entry` rejects a
non-lowercase `LANG` loudly rather than branching on it. This was the only
instance of the class.
