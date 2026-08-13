---
id: 539
title: "Fixture naming has four independent drifts: two spellings of the lang axis, no rule for build/<kind>, three schemes for zephyr build dirs, phase-coded manifest ids"
status: open
type: tech-debt
area: build, testing
related: [issue-0535, rfc-0070, phase-334]
---

## Problem

RFC-0070 / phase-334 W2.b settled the build-cache ROOT — every path now derives
from `nros_build_dir <kind>` and "zero rooted cache literals remain". It never
covered the VOCABULARY of the names passed to it, or the names fixtures carry
elsewhere. Four independent drifts remain.

### 1. The language axis has two spellings

| context | spelling |
| --- | --- |
| `examples/fixtures.toml`, `matrix::Lang`, `fixtures-manifest.py` | `rust` `c` `cpp` |
| zephyr build dirs, via `nros_zephyr_lang_tag` (`fixture-matrix.sh:29`) | `rs` `c` `cpp` |

So one leaf is `rust` to the manifest and `rs` on disk
(`zephyr-workspace/build-rs-talker-zenoh`). Any join between the two halves —
which is what issue 0535 needs — has to know both, and a translation table
between two spellings of one axis is the shape that produced issue 0482.

### 2. `build/<kind>` has no rule

The 14 kinds in use, as passed to `nros_build_dir`:

```
borrowed-e   cargo        cmake-fixtures  compile-check
fixture-make-driver       fixtures-cargo  idf-fixtures
link-determinism          px              sizes-probe
tools        west-fixtures
zephyr-fixture-build      zephyr-fixture-make-driver
```

Three separate inconsistencies:

* `cmake-fixtures` / `idf-fixtures` / `west-fixtures` are `<tech>-fixtures`, but
  `fixtures-cargo` **reverses the order** and `compile-check` drops the suffix
  entirely — for a family that is a fixture kind in exactly the same sense.
* `zephyr-fixture-build` and `zephyr-fixture-make-driver` are two roots for one
  family, distinguished by which script writes them, not by what they hold.
* `borrowed-e` and `px` are truncations of nothing legible.

### 3. Zephyr build dirs use three unrelated schemes at once

From `ls -d zephyr-workspace/build-*` (75 dirs):

| scheme | example | count |
| --- | --- | --- |
| `<lang>-<role>-<rmw>` | `build-rs-talker-zenoh` | 54 |
| `ws-<lang>[-<feature>]-entry-<rmw>` | `build-ws-rs-qos-entry-zenoh` | 12 |
| ad-hoc | `build-fvp-aemv8r`, `build-aemv8r`, `build-s32z-board-import`, `build-cortex-m-c-talker` | rest |
| **issue-numbered** | `build-245-asan`, `build-245-heapval` | 2 |

The last row is the one CLAUDE.md already bans for test identifiers ("Test names
describe behavior, not phase numbers [...] Phases go stale"). The same argument
applies verbatim to a build dir, which outlives the issue by longer.

### 4. Manifest ids mix conventions, and some are phase-coded

From `fixtures-manifest.py list-compile-checks`:

```
n9_form1  n9_form2  n9_form3  n9_form4      # phase-coded
o4_pkg_index  o5_nav2_compat  o3_board_agnostic
l9_register_c  l9_register_cpp
cpp_robot_entry  c_mixed_workspace  shadowing   # snake, descriptive
freertos-logging-smoke-mps2  threadx-linux-logging-smoke   # kebab, descriptive
```

Both `_` and `-` as the separator, and nine ids whose meaning is a work-item
letter from a phase nobody has open. `--id` is a user-facing selector
(`fixtures-build.sh --id freertos-logging-smoke-mps2` appears in three
recipes), so these are typed by hand.

## Why it is worth fixing rather than tolerating

Issue 0535 wants the 74 west fixtures to become manifest rows. That join is
`(platform, lang, rmw)` on one side and `<lang>-<role>-<rmw>` path segments on
the other — with `rust`/`rs` disagreeing. Renaming after the migration means
touching every path a second time; renaming before it means the migration writes
one vocabulary once.

## Direction

1. Pick the manifest spelling (`rust`) as the SSoT for the lang axis and delete
   `nros_zephyr_lang_tag`, or keep the tag but make it derive from
   `matrix::Lang` so there is one producer.
2. State the `build/<kind>` rule in RFC-0070 (`<family>-fixtures` for fixture
   trees, bare `<family>` otherwise) and rename the five outliers.
3. One scheme for zephyr build dirs, derived from the row's coordinate once
   0535 lands — the dir name then has a producer instead of a convention.
4. Ban phase/issue-coded ids in fixture ids the way test names already are,
   with a gate, since a convention without one is what produced this list.

Sequence after 0535's rows exist but before its scripts are cut over, so the
cutover writes the final names.
