---
id: 1523
title: "`NROS_COMPONENT_LANG` is written lowercase by one verb and uppercase by
  another, and all three readers test `STREQUAL \"C\"` — so a
  `nano_ros_auto_add_library` C component never takes a C branch"
status: open
type: bug
area: [cmake, build]
severity: low
found: 2026-09-28
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
