---
id: 1253
title: "A Zephyr workspace compiles the nano-ros MODULE of whichever checkout
  provisioned it — tier 2 built another checkout's headers beside this tree's
  entries, and judged images no run of this tree made"
status: open
type: bug
area: ci, zephyr, build
related: [issue-1158, phase-440, phase-431, rfc-0095, rfc-0079, phase-449]
---

## Problem

`run-matrix` (tier 2) produced no verdict for eight straight runs; the last
seven stopped in `just build tier2`, in the Zephyr family. They did not all stop
on the same thing, and none of the failure texts named the real cause:

| run (2026-09) | SHA | stopped at |
| --- | --- | --- |
| 06, 06, 07 | efb6db9 / a7c92da / 055b843 | `this nros does not belong to the checkout` — the phase-431 W1 guard, checkout `/mnt/evo/…/nano-ros` |
| 08, 08, 10 | 3e96069 / 6e7779b / **b5097a4** | `tier-priority-plan-image: FAILED (160 pin-check(s) over 40 image(s))` after every leaf BUILT |
| 09 | **b5097a4** | `build-cpp-action-server-xrce` compile, error line not in the printed tail |

Same SHA, two different failures (09 vs 10): the lane's outcome depended on
something outside the commit under test.

That something is the runner's Zephyr workspace. It sits at
`/mnt/evo/<user>/nano-ros/zephyr-workspace` — inside a SECOND nano-ros
checkout — and the runner's checkout is `/home/<user>/actions-runner/_work/…`.
Three consequences, only the first of which anything checked:

1. **The CLI ownership guard** (phase-431 W1) refuses a `nros` from one tree
   operating inside the other. `check-zephyr-workspace-checkout.sh` (f33c584)
   front-runs it — but mirrors its silent cases exactly, including
   `NROS_SKIP_STALE_CHECK=1` and "that checkout has no `packages/cli`". From
   2026-09-08 neither the guard nor the front-run fired on the runner, so one of
   those two became true there, and the lane went on building.

2. **The nano-ros Zephyr MODULE comes from the workspace's west manifest, not
   from the checkout that runs the build.** `.west/config` says
   `[manifest] path = nano-ros`, a symlink to whichever checkout ran
   `west init -l`; west lists it as the `nros` module
   (`build-*/zephyr_modules.txt`: `"nros":"<ws>/nano-ros":"<ws>/nano-ros/zephyr"`),
   and `zephyr/CMakeLists.txt` sets `NROS_REPO_DIR` to that module's parent. So
   every image compiled `/mnt/evo`'s `zephyr/`, platform sources and nros-cpp
   headers next to entry code the runner's CLI generated. Run 34319241943
   printed `/mnt/evo/…/packages/api/nros-cpp/include/nros/main.hpp` inside
   `build-cpp-action-server-xrce`, and failed; the next night, same SHA, it
   built. A mixed tree is not a result about either tree. Nothing asked this
   question — it does not depend on the CLI guard at all.

3. **`check-tier-priority-plan-image.py` judged every `build-*/` in the
   workspace**, not the images the run built. A shared, long-lived workspace
   holds images from other trees and older commits; those still carrying the
   pre-issue-0852 zenoh band (`CONFIG_NROS_ZENOH_READ_PRIORITY=16` on the old
   0-31 scale → transport `[14, 14]`) fail the realtime workspaces' tier pins
   (`9`, `10`), which pass against every freshly built image (`[4, 4]`,
   pool `[5, 14]`). Reproduced on the dev host: its 72 images date from
   2026-08-28 and ALL fail discovery mode, including leaves with no tiers.

And behind all three, the printers showed tails only: the fixture fan-out's
`tail -40` and the Zephyr scheduler's `tail -n 80`. Under a parallel ninja the
failing unit's `error:` scrolls above the warnings every other in-flight unit
keeps printing, so run 34319241943's "log tail" was eighty lines of
`-Wunused-result` notes and no error. Issue 1158's rule — a lane's failure text
must name the failure — one level down.

## What landed (fix/tier2-zephyr-cpp-build)

- `check-zephyr-workspace-checkout.sh` asks the MODULE question too: it
  resolves the workspace through the one resolver (`scripts/lib/zephyr-workspace.sh`),
  reads its west manifest, and refuses one that resolves to a different
  nano-ros checkout. `NROS_SKIP_STALE_CHECK=1` does not silence that half — it
  is not the CLI guard's question. Runs at the head of
  `check-tier-preconditions`, so the runner now stops there, naming both paths,
  instead of 15-25 minutes into the build with a mixed-tree answer.
- `check-tier-priority-plan-image.py --images-from <list>`: the Zephyr lane
  passes the build dirs of the records it just built (field 12). Discovery mode
  stays for the operator's `just check tier-priority-plan-image`. A listed dir
  with no `.config` is an error, not a skip. The summary line names the failing
  images (the per-image `[FAIL]` blocks scroll above any tail).
- `scripts/build/log-first-errors.sh`, called by all THREE fixture failure
  printers before their tails: the fan-out in `justfile`, the Zephyr leaf
  scheduler, and `scripts/build/fixture-make-driver.sh` (found by sweeping
  `tail -n` in the fixture scripts, not reported).
- `zephyr/cmake/nros_cargo_build.cmake`: stale `NROS_RESOLVED_*` knob values
  are cleared at the top of every configure — see below.
- `main.hpp`: the ten `nros::shutdown()` calls that discarded a `[[nodiscard]]`
  `nros::Result` (the warnings that filled the 09-09 tail) are `(void)`-cast.

## The failure behind the first one (issue 1070's shape)

Rebuilding tier 2's seven Zephyr leaves on a dev host whose workspace IS its
own checkout, with the fixes above, the six `cpp-*-xrce` leaves built and
`build-cortex-m-c-talker-zenoh` failed its configure:

    CONFIG_MAX_PTHREAD_MUTEX_COUNT=32 is too small for 8 subscribers.
      need at least 34 = 8 subscribers + 22 zenoh-pico fixed + 4 headroom

— two lines after the same configure logged `NROS_RMW_SUBSCRIBER_SLOTS left to
its crate default -- no value stated and none derivable`. The 8 was a
2026-08-28 configure's: `_nros_resolve_knob` stores every value
`CACHE INTERNAL`, `nros_resolve_knobs` cleared only the LIST of knobs, and
`_nros_resolve_derivable_knob`'s rung 4 deliberately resolves nothing — so a
knob that was stated or derived once and is neither now kept its old
`NROS_RESOLVED_<knob>` forever in a reused build dir. The phase-412 mutex floor
(822538ed5, 2026-09-05) is the first reader that FAILS on it; every other
reader silently used the stale number. Fixed by clearing every INTERNAL
`NROS_RESOLVED_*` cache entry at the top of `nros_resolve_knobs`, enumerated
from the cache (a configure that dies mid-way leaves values with no list
naming them). Verified in place, no wipe: re-configure clears the 8 (25 → 20
entries), the floor stays quiet, and the leaf links.

## What still needs a human

The runner itself. Per `check-zephyr-workspace-checkout.sh`, the sanctioned fix
moves the JOB, not the directory: run it contained
(`just runner-up nros-qemu,nros-sdk-zephyr,nros-big`), where
`runner-provision.sh` provisions the workspace INTO the runner's own checkout.
Until then tier 2 fails at preconditions — loudly, with the paths — which is the
honest state: it has been certifying `/mnt/evo`'s module, not the commit.

Also unshared by construction: the Zephyr fixture build lock
(`zephyr-fixture-make-driver.sh`, issue #19) lives under the CHECKOUT's
`build/`, so two checkouts building into one workspace do not serialise on the
same `build-*` dirs at all.

## Option not taken

`zephyr_module.py` keys modules by NAME and processes `ZEPHYR_EXTRA_MODULES`
after west's list, so `-DZEPHYR_EXTRA_MODULES=<this checkout>` would re-bind
the `nros` module to the tree under test even with a foreign manifest. Not done
here: the build dirs would still live inside the other checkout (so the CLI
guard still refuses, correctly) and the lock would still not serialise, so it
would fix one of three consequences while making the shared layout look
supported. Worth revisiting only if a shared workspace becomes a supported
configuration (RFC-0095's store-resolved workspace is the likelier home).
