---
id: 1263
title: "Board facts never reached a Zephyr image: the CLI was looked up under
  one of its two names, failures were cached for good, and the CLI was never
  told where the checkout is"
status: resolved
type: bug
area: zephyr, build
severity: medium
resolved_in: "fix(#1263): board facts ask the shared CLI resolver, and do not cache its absence"
related: [issue-1261]
---

## Symptom

Every Zephyr configure printed

```
-- nano-ros: board facts NOT delivered -- no nros CLI (build it with
   `./scripts/bootstrap.sh`; contributors: `just setup-cli`).
```

with the pinned in-tree CLI built and first on PATH. Measured on a
downstream (Autoware Safety Island): its MR-CANHUBK344 image and its
native_sim image both printed it on every fresh configure, so neither has ever
been built with its board facts. It is a STATUS line and the build succeeds,
so nothing drew attention to it.

## Cause

Three defects, each hiding the next: fixing the lookup exposed the cache,
and clearing the cache exposed the missing checkout path.

**The lookup knew one name.** `cmake/NanoRosBoardFacts.cmake` checked
`_NANO_ROS_CODEGEN_TOOL` and nothing else. The CLI goes by two cache names --
`_NROS_ZEPHYR_CODEGEN_TOOL` on the Zephyr lane -- and
`cmake/NanoRosImageAgreement.cmake` already records why a check knowing only
one of them "would silently do nothing on the lane it was written for". On
Zephyr, `zephyr/CMakeLists.txt` calls `nros_resolve_board_facts()` right after
including `nros_cargo_build.cmake`, before either name is set and before
anything has loaded `nros_resolve_cli`, the shared resolver that tries
`$NROS_CLI`, both cache names and then PATH. So copying ImageAgreement's
`if(COMMAND nros_resolve_cli)` guard would have skipped on exactly this lane.

**The absence was cached.** The "no CLI" verdict was memoized in the cache
(`NROS_BOARD_FACTS_ENV__<board>__<deploy>`, help string
"phase-351 W5: no CLI"), and the memo is consulted before the lookup. A build
dir that once configured without the CLI therefore never asked again: after
fixing the lookup alone, the downstream's existing build dirs still returned
an empty answer, now without even the STATUS line. Both of its build dirs held

```
//phase-351 W5: no CLI
NROS_BOARD_FACTS_ENV____:INTERNAL=
```

It was not only "no CLI". Every failure path cached an empty answer: no
workspace, and any CLI error other than the netstack domain check
("phase-351 W5: nothing to deliver"). With the lookup fixed and the "no CLI"
entry dropped, the CLI ran, failed for the reason below, and that failure was
cached in its place.

**The checkout was never named.** `nros ws board-facts` finds the nano-ros
checkout from `--nano-ros-path`, then `$NROS_REPO_DIR`, then by searching
upward from the entry dir (`cmd/board_facts.rs`). The cmake passed no
`--nano-ros-path`; the Zephyr lane sets `NROS_REPO_DIR` only as a cmake
variable, and only after this call (`zephyr/CMakeLists.txt`); and a
downstream project keeps nano-ros BELOW it (`third-party/nano-ros`), where an
upward search cannot reach. So the first run that got past the other two
defects printed

```
-- nano-ros: board facts NOT delivered from .../src/zephyr_entry --
   Error: no nano-ros checkout found (pass --nano-ros-path)
```

## Fix

- `NanoRosBoardFacts.cmake` includes `NanoRosCodegenCore.cmake`
  (`include_guard(GLOBAL)`, functions only at top level) and resolves the CLI
  with `nros_resolve_cli(... OPTIONAL)`, running what it returns.
- Only a RESOLVED answer is cached across configures. A failure describes the
  build environment of one configure, not the board, so it is recorded in a
  GLOBAL property (the several callers of one configure still ask once), and
  a cached failure left by an older configure is dropped on read.
- The call passes `--nano-ros-path`, the checkout this cmake file belongs to,
  resolved at file scope (inside the function `CMAKE_CURRENT_LIST_DIR` names
  the caller).

Sweep for other single-name lookups:

```
git grep -n -E '_NANO_ROS_CODEGEN_TOOL|_NROS_ZEPHYR_CODEGEN_TOOL' -- cmake zephyr
```

Every other reader goes through `nros_resolve_cli`,
`_nros_resolve_codegen_tool`, or checks both names.

## Effect on the downstream

With all three fixed, the CLI runs on both of the island's Zephyr images and
reports why it has nothing to deliver, instead of claiming there is no CLI:

```
Error: no system.toml at .../src/zephyr_entry or .../src/zephyr_entry/src/*/
```

That is the correct answer for this image: its entry is a C++ leaf and its
board is named the Zephyr way, so it declares no deploy for board facts to
resolve. Both images are unchanged -- MR-CANHUBK344 DTCM 83,192 B and
`ARENA_SIZE` 50,640 before and after, native_sim builds -- and neither build
dir holds a board-facts cache entry any more.

## Not fixed here

`nros ws board-facts` takes a workspace root (`src/*/system.toml`) or a dir
holding `system.toml`, and the Zephyr lane passes the entry leaf
(`APPLICATION_SOURCE_DIR`). A Rust entry leaf is found through its
`[package.metadata.nros.entry] deploy`; a C++ entry leaf, whose bringup is
named by `nano_ros_add_executable(... BRINGUP <pkg>)`, is not, and the verb has
no flag to name a bringup. So a C++ Zephyr image that DOES declare a deploy
still gets no board facts. That is the same missing route as issue 1261 -- the
Zephyr lane does not resolve anything from the entry's BRINGUP -- and belongs
with it.
