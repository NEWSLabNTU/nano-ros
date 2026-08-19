---
id: 698
title: "Every SDK-toolchain Zephyr board fails to configure under CMake 4: Zephyr 3.7's SDK lookup has a malformed `if()` that only an unset `ZEPHYR_TOOLCHAIN_VARIANT` reaches"
status: resolved
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

## Fix applied (2026-08-19) — direction 1, two sites, NOT verified end-to-end here

`ZEPHYR_TOOLCHAIN_VARIANT=zephyr` is now stated explicitly for non-`native_sim`
boards in BOTH call sites, per the warning above that one alone is the half-fix
class:

* `scripts/build/zephyr-fixture-run-one.sh` — a `*)` arm beside the existing
  `native_sim → host` arm, inside the same "caller override wins" guard.
* `scripts/build/west-fixtures.sh` — the same branch. Its EMPTY-board case (the
  FVP `board_import` entry, which resolves through `board.cmake`) is left
  deliberately alone: it is not a board name this `case` can key on, and the
  measurements in this issue cover named SDK boards only.

**What was verified on the machine that applied it, and what was not.** That
host has CMake **3.22.1** and no Zephyr checkout or SDK, so it can reproduce
neither the failure nor a real Zephyr configure:

* VERIFIED — behaviour-neutral on CMake 3. On this issue's own three-line
  snippet, unset and `=zephyr` both print `TOOK-THE-BRANCH`, so the change
  cannot alter which branch a working host takes. That is the regression half.
* NOT VERIFIED — that it clears the CMake 4 error. The mechanism and the remedy
  are established by the measurements ABOVE (`-DZEPHYR_TOOLCHAIN_VARIANT=zephyr`
  → passes; the real `mps2_an385` configure gets past the toolchain stage), but
  those were taken by the filer, not re-run here.

Left OPEN for that reason. Closing it wants one `just zephyr build-fixtures` for
a real board on a CMake ≥ 4 host — which is also the run that proves tier 2 is
unblocked.

## Fixed 2026-08-19 — name the variant, so ONE tree serves both CMake lines

Took direction 1. `zephyr` is exactly the value Zephyr would have chosen for
these boards, so naming it is not a version switch: both CMake lines take the
same branch and reach the same code. That property is the requirement, not a
bonus — Ubuntu 22.04 ships CMake 3.22 and ROS Humble is bound to it, so the fix
has to keep working there rather than merely unblock a rolling host.

One helper, `scripts/build/zephyr-toolchain.sh`, for all three callers that had
the rule spelled separately. An externally-set variant still wins at every site,
so a third-party toolchain keeps working.

Two corrections to the section above, which was written concurrently and landed
first:

* **Three sites, not two.** `just/zephyr-dev.just` carries the same
  `native_sim -> host, everything else unset` case and was missed, so
  `just zephyr build` for a real board stayed broken on CMake 4. That is the
  half-fix class this issue warned about, arriving one site further along than
  the warning reached — which is the argument for a helper over a third
  hand-written `case`.
* **The empty board must say `zephyr` too.** The FVP `board_import` entry was
  deliberately left unset on the grounds that its board name is not something
  the `case` can key on. But the failing `if()` does not care WHICH board is
  being built — it fails whenever the variant is unset, so that row is still
  dead on CMake 4. The helper's `*)` arm covers it, which is correct precisely
  because that entry was always SDK-gated.

### Verified — the full 2x2, each cell a real `mps2_an385` configure

| CMake | variant | result |
| --- | --- | --- |
| 3.22.1 | unset (old) | `Found toolchain: zephyr 0.16.8` |
| 3.22.1 | `zephyr` (new) | `Found toolchain: zephyr 0.16.8` |
| 4.4.2 | unset (old) | `FindZephyr-sdk` `if()` error |
| 4.4.2 | `zephyr` (new) | `Found toolchain: zephyr 0.16.8` |

The 3.22 row that matters is the first one: it shows the fix CHANGED NOTHING on
the version that already worked. Only the `ZEPHYR_TOOLCHAIN_VARIANT not set,
trying to locate Zephyr SDK` status line disappears, because the variant is now
set.

Repeatable on any host: `scripts/zephyr/cmake-variant-probe.sh` probes one SDK
board and one native_sim and keys its verdict ONLY on the SDK-lookup outcome, so
a configure that fails later for unrelated reasons (stale CLI, missing generated
interfaces) does not muddy the answer.

And end to end, through the real fixture path on CMake 4.4.2 — the lane that
could not configure at all:

```
== zephyr == OK
```

### Fell out of testing the 3.22 half

The ROS distrobox is where a CMake 3.22 lives here, and it could not build ANY
Zephyr target — `ros2-box-sync.sh` excludes `build/` at any depth, which strips
Zephyr's four SOURCE directories named `build` (`scripts/build`,
`scripts/tests/build`, `doc/build`, `share/sysbuild/build`). Same class the sync
script's own comment documents, recurring because its re-include is anchored at
the repo root and `zephyr-workspace` is gitignored, so the `git ls-files`
reasoning never covered it. The symptom names the wrong thing:

```
python3: can't open file '.../zephyr/scripts/build/dir_is_writeable.py'
CMake Error at .../boards.cmake:198: Error finding board: mps2
```

Fixed in the same change; costs ~2 MB. The box also needs its own Python
environment for Zephyr (the mirrored in-tree venv is the host interpreter) —
the probe now checks for `pykwalify`/`PyYAML`/`pyelftools` up front and says so,
instead of surfacing four frames deep as a board error.

### Not done here

Zephyr 3.7 keeps the unquoted `if()`. Direction 3 (move off 3.7) remains where
this should end up; it is blocked as issue 0651 describes.

## Not the same as issue 0651

0651 is about the 4.4 line being nightly-only, so a Kconfig or API change lands
unverified. This is the 3.7 line failing outright on a modern host. They meet in
direction 3 and are otherwise independent.
