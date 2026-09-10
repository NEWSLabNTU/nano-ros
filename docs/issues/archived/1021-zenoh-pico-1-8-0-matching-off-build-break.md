---
id: 1021
title: "zenoh-pico 1.8.0 does not compile with `Z_FEATURE_MATCHING=0`: an
  unguarded call to a MATCHING-only function"
status: resolved
type: bug
area: rmw, third-party
related: [phase-415, phase-444, issue-0910]
---

## What

Upstream zenoh-pico `1.8.0` fails to compile when `Z_FEATURE_MATCHING=0`:

```
src/net/filtering.c:330:5: error: implicit declaration of function
  '_z_write_filter_ctx_remove_callbacks';
  did you mean '_z_write_filter_ctx_remove_local_match'?
```

`_z_write_filter_clear` is unguarded, but the function it calls is declared
(`include/zenoh-pico/net/filtering.h:95`) and defined (`src/net/filtering.c:342`)
inside `#if Z_FEATURE_MATCHING`.

**This is upstream's, not ours.** Pristine `1.8.0` has the same unguarded call
at `filtering.c:323`, and the one commit on our patch line that touches this
file (`98ab67c4 fix: open filters for single-threaded clients`) adds no call
to it.

## Why it matters here

`Z_FEATURE_MATCHING=0` is exactly what nano-ros passes on Zephyr
(`zephyr/cmake/nros_rmw_zenoh.cmake`: `zephyr_compile_definitions(Z_FEATURE_INTEREST=1
Z_FEATURE_MATCHING=0)`). So every Zephyr zenoh image fails to build `libnros`
against a pristine 1.8.0. It surfaced the moment phase-415 moved the patch line
to 1.8.0. Native builds do NOT catch it — every other lane compiles
MATCHING=1 (`nros-zpico-build`'s `config_header()` writes it as a constant), so
the guarded definition is present and the unguarded call resolves.

## Fix carried on the patch line

`0343ad1b` on `nano-ros` guards the call:

```c
#if Z_FEATURE_MATCHING == 1
    _z_write_filter_ctx_remove_callbacks(_Z_RC_IN_VAL(&filter->ctx));
#endif
```

The guard is right rather than merely expedient: the state it clears,
`_z_write_filter_ctx_t::callbacks` (`net/filtering.h:58`), exists only under
`Z_FEATURE_MATCHING == 1`. With the feature off there is nothing to remove, so
skipping the call leaks nothing.

## Resolution (phase-444 W5, 2026-09-11) — does not reproduce at the pin

Measured, not inferred:

| fact | value | how |
| --- | --- | --- |
| superproject pin | `dd071b8d` | `git ls-tree HEAD packages/rmw/zenoh/zpico-sys/zenoh-pico` |
| its version | **1.8.0** (not the 1.7.2 CLAUDE.md/AGENTS.md said — both corrected) | `version.txt` |
| pin on the patch line | yes, `dd071b8d` is an ancestor of `origin/nano-ros` (`97557b00`) | `git merge-base --is-ancestor` after widening the refspec |
| fix in the pin | yes, `0343ad1b` is 7 commits below `dd071b8d` | `git log --oneline dd071b8d` |
| upstream | fixed by `07c84ebc` "Fix feature guards and warning for modular builds (#1225)", 2026-05-20 — NOT in 1.8.0 (`eb47e2e0`, 2026-03-13), IN 1.10.0 (the fork's `main`, `e621319b`), same guard at `filtering.c:323` | `git log -S` on `origin/main` |

Host syntax sweep over every portable TU (`src/{api,collections,link,net,
protocol,session,transport,utils,system/common}` + `system/unix`, 132 TUs),
`gcc 16.1.1 -std=gnu11 -fsyntax-only -Werror=implicit-function-declaration
-DZENOH_LINUX -Iinclude`:

```
pin dd071b8d,           -DZ_FEATURE_MATCHING=0 : 132 TUs, 0 failed
pin dd071b8d,           -DZ_FEATURE_MATCHING=1 : 132 TUs, 0 failed
0343ad1b~1 (0844ab0f),  -DZ_FEATURE_MATCHING=0 : 132 TUs, 1 failed
  src/net/filtering.c:330:5: error: implicit declaration of function
  '_z_write_filter_ctx_remove_callbacks'; did you mean '_z_write_filter_callback'?
```

So it is **resolved by the pin**, and it does **not** belong to issue 0910:
upstream 1.10 carries the same guard, so the migration cannot bring it back.

### Open item 2 — the MATCHING sweep

Answered by compiling rather than by grepping declarations: with
`Z_FEATURE_MATCHING=0` every portable TU compiles at the pin, so no other
MATCHING-only symbol is reachable from unguarded code. `_z_write_filter_clear`
was the only site.

### Open item 3 — the other feature-off axes

Same sweep at the pin, each axis on its own:

```
Zephyr set, TX_SPLIT_LOCK=1 (INTEREST=1 MATCHING=0 LOCAL_SUBSCRIBER=1) : 0 failed
Zephyr set, TX_SPLIT_LOCK=0                                            : 0 failed
SCOUTING=0                                                             : 0 failed
RAWETH_TRANSPORT=0                                                     : 0 failed
MULTICAST_TRANSPORT=0 SCOUTING=0 LINK_UDP_MULTICAST=0                  : 0 failed
MULTI_THREAD=0 MATCHING=0                                              : 0 failed
LINK_UDP_UNICAST=0 LINK_UDP_MULTICAST=0 (scouting left ON)            : 1 failed
  src/session/scout.c:39:47: error: 'UDP_SCHEMA' undeclared
```

The last one is a second latent upstream break of the same shape (`scout.c`
uses `UDP_SCHEMA` under `Z_FEATURE_SCOUTING` alone; upstream 1.10.0 is
identical). **No nano-ros build reaches it**: the cargo lane writes
`Z_FEATURE_SCOUTING` from `multicast_transport_flag()` (off by default, issue
0682), so scouting is off whenever UDP is, and the Zephyr lane keeps
`config.h`'s UDP defaults (on). Recorded here, not fixed on the fork — a fix
for a configuration nobody builds is unmeasurable here. If a lane ever turns
both UDP links off with scouting on, this is the error it will hit.

### Guard against a future bump

`just check zenoh-feature-off-compile` (`scripts/check-zenoh-feature-off-compile.py`)
syntax-checks every TU the shared manifest (`zpico-sys/zenoh-sources.txt`)
compiles unconditionally, in each configuration the Zephyr lane's
`zephyr_compile_definitions` really passes — derived from the cmake file, so a
changed Zephyr flag is compiled, not a stale copy of it — and requires
`Z_FEATURE_MATCHING=0` to stay covered. Its self-test undoes `0343ad1b` in a
temp copy of `filtering.c` and requires this issue's diagnostic back, and
proves the compiler treats implicit declarations as errors (GCC < 14 only
warns). It needs the zenoh-pico source, so it is a recorded skip on the
source-free push lane and runs where `gate.yml` provisions zenoh-pico
(pull_request / merge_group). ~6 s wall.

### Still open, out of this repo

Report upstream is moot for MATCHING (`07c84ebc` is upstream's own fix). The
`scout.c` / UDP-off case above has not been reported.
