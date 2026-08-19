---
id: 698
title: "Every SDK-toolchain Zephyr board fails to configure under CMake 4: Zephyr 3.7's SDK lookup has a malformed `if()` that only an unset `ZEPHYR_TOOLCHAIN_VARIANT` reaches"
status: open
type: bug
severity: high
area: build/zephyr
related: [issue-0087, issue-0651, phase-196]
---

## Symptom

Every Zephyr fixture for a REAL board (`mps2_an385`, the FVP cortex-a/r boards,
the cyclonedds targets) dies at configure:

```
CMake Error at zephyr-workspace/zephyr/cmake/modules/FindZephyr-sdk.cmake:35 (if):
  if given arguments:

    "(" "zephyr" "STREQUAL" ")" "OR" "(" "NOT" "DEFINED" "ZEPHYR_TOOLCHAIN_VARIANT" ")"
    "OR" "(" "DEFINED" "ZEPHYR_SDK_INSTALL_DIR" ")" "OR" "(" "Zephyr-sdk_FIND_REQUIRED" ")"

  Unknown arguments specified
```

`native_sim` is unaffected, which is why this is invisible until an embedded
board is built.

## Blast radius: it takes tier 2 with it

`just ci-matrix` is 1-wise over platform, so every platform is in it (#357,
#482). The zephyr lane failing at configure fails the tier:

```
make: *** [tmp/build-test-fixtures-.../build-test-fixtures.mk:8: zephyr] Error 2
error: recipe `build-test-fixtures-leaves` failed with exit code 2
```

So on a host with CMake ≥ 4 there is no tier 2 at all — not a narrowed tier, no
tier. Tier 1 is native-only and cannot see it.

## Cause

`FindZephyr-sdk.cmake:35` interpolates the variable **unquoted**:

```cmake
if(("zephyr" STREQUAL ${ZEPHYR_TOOLCHAIN_VARIANT}) OR
   (NOT DEFINED ZEPHYR_TOOLCHAIN_VARIANT) OR ...
```

When `ZEPHYR_TOOLCHAIN_VARIANT` is unset the reference expands to nothing and
the condition becomes `if("zephyr" STREQUAL )` — missing its right operand.
CMake 3.x tolerated that; CMake 4 rejects it.

**Controlled, on the same three-line snippet, same machine:**

| CMake | result |
| --- | --- |
| 3.22.1 (Ubuntu 22.04, in the ROS distrobox) | `-- TOOK-THE-BRANCH` |
| 4.4.2 (Arch, host) | `Unknown arguments specified` |

Two more measurements that pin the mechanism down:

* `-DZEPHYR_TOOLCHAIN_VARIANT=zephyr` → passes. The variable being DEFINED is
  what makes the line parse.
* `-DZEPHYR_SDK_INSTALL_DIR=/x` alone → still fails. The whole argument list has
  to be well-formed before any clause is evaluated, so the later
  `(DEFINED ZEPHYR_SDK_INSTALL_DIR)` clause cannot rescue the first one. Worth
  stating because "point it at the SDK" is the obvious first thing to try, and
  it does nothing.

Unset is the DESIGNED state for these boards. `scripts/build/zephyr-fixture-run-one.sh`
sets `ZEPHYR_TOOLCHAIN_VARIANT=host` for `native_sim` only, and says so:

```
# Real embedded boards (FVP cortex-a/r, cyclonedds targets) leave the variant
# unset → Zephyr locates the downloaded SDK as before.
```

That was correct under CMake 3 and is the exact path CMake 4 now rejects — so
the native_sim carve-out from issue 0087 is also what has been hiding this.

## Fix directions

1. **Set `ZEPHYR_TOOLCHAIN_VARIANT=zephyr` for SDK boards** — the sibling of the
   `native_sim → host` branch already in `zephyr-fixture-run-one.sh`, in the same
   board-keyed `case`, plus the copy in `scripts/build/west-fixtures.sh` (both,
   or it is the half-fix class). Verified to clear it: with the variant set, the
   real `mps2_an385` configure gets past the toolchain stage and on into
   `NanoRosVerbs.cmake`. Cheapest, and it makes an implicit default explicit —
   which is the thing that broke.
2. **Keep a CMake 3 for Zephyr builds.** Restores upstream's assumption rather
   than working around it, at the cost of a second cmake to provision and a
   version skew between lanes.
3. **Move off Zephyr 3.7.** Upstream's own fix. Blocked the same way issue 0651
   describes — the 4.4 line is reachable only from nightly.

Direction 1 is the one to take; 3 is where it should end up.

## Not the same as issue 0651

0651 is about the 4.4 line being nightly-only, so a Kconfig or API change lands
unverified. This is the 3.7 line failing outright on a modern host. They meet in
direction 3 and are otherwise independent.
