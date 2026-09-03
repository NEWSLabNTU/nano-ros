# Phase 415 — the zenoh-pico patch line moves to 1.8.0

**Status (2026-09-03). SURVEYED, not started.** The backup exists and is pushed;
the port itself is a merge, not a replay, and the measurement below is why it is
a phase rather than an afternoon.

Carries [issue 0910](../issues/0910-zenoh-pico-1-10-migration.md), whose target
changes here: **1.8.0, not 1.10.** 0910 is written around 1.10 and its blocker
(the config generator) is a 1.10 problem.

## Why 1.8.0

Measured in the `ros2` distrobox on 2026-09-03, apt index refreshed:

```
#define ZENOH_C "1.8.0"     /opt/ros/humble/opt/zenoh_cpp_vendor/include/zenoh_configure.h
ros-humble-rmw-zenoh-cpp    Installed: 0.1.9-1jammy.20260723.022609
                            Candidate: 0.1.9-1jammy.20260723.022609
```

Installed EQUALS candidate, so 0.1.9 is the newest Humble ships, built
2026-07-23 and unchanged since. The rule is to track the zenoh ROS adopts:

| | zenoh |
| --- | --- |
| ROS Humble today | **1.8.0** |
| our `nano-ros` line | 1.7.2 — one minor behind |
| `nros-integration-1.10` | 1.10.0 — two minors AHEAD of ROS |

The 1.10 integration branch is further from ROS than we are, so it is the wrong
target under that rule. Nothing on it ports anyway:

* `f59939e5` (generated `config.h`) is 1.10-only by construction — 1.10 makes
  `config.h` CMake-generated from `config.h.in`; at 1.7.2 it is not generated.
* `1f41f817` (`Z_HAS_SOCKET_LINK`) targets `link/transport/socket.h`,
  `address.c`, `endpoints.c` — **none exist at 1.7.2**. Its shape is better than
  ours (it reaches stubs `address.c`/`endpoints.c` already had but could not
  select); our line solves the same problem locally in `348c21e8`.
* `e64e099b` (bounded serial read) — **our line is AHEAD.** Ours already has the
  deadline, the `k_yield()` AND interrupt-driven RX from issue 0852. Porting it
  back would regress us.

## The measurement — this is a merge

Applying our delta (`git diff upstream-1.7.2 <root>`) onto `upstream-1.8.0`:

```
85 files apply cleanly
42 files conflict
 2 files upstream MOVED   protocol/keyexpr.{h,c} -> session/keyexpr.{h,c}
```

Conflicts by area — not peripheral, this is the session layer:

```
8  src/session/          (interest, liveliness, query, queryable, resource,
                           subscription, utils, loopback)
5  include/zenoh-pico/session/     5  include/zenoh-pico/net/
4  src/api/              4  tests/
3  src/transport/        (both lease paths, unicast read)
3  src/net/              2  include/zenoh-pico/protocol/
2  include/zenoh-pico/api/  (types.h, constants.h)
1  config.h              1  CMakeLists.txt   1  collections/refcount.c
```

The arithmetic explains it: our delta is 6,448 insertions over 138 files;
upstream's 1.7.2 -> 1.8.0 churn is 15,534 insertions over 186 files, and they
overlap where 1.8.0 restructured the session layer — exactly where our admin
space, liveliness and link work lives.

## The collision that needs a DECISION, not a merge

Our delta ADDS `include/zenoh-pico/session/keyexpr.h`. Upstream 1.8.0 MOVED its
own `protocol/keyexpr.h` to that same path. **Same filename, different content,
different purpose.** This is not a textual conflict a three-way merge can
settle; someone has to decide which file owns the name and rename the other,
then fix every include.

This is also why `git apply -3` fails atomically on the whole patch: two paths
"do not exist in index" at 1.8.0, and rewriting the paths by hand loses the
rename detection that would have made the rest tractable.

## What our patch line actually contains

17 files ADDED on top of 1.7.2, in five groups:

* **admin space** — `api/admin_space.{h,c}` + test; `admin_space` appears in 32
  files, so it is cross-cutting rather than a leaf
* **IVC link** — `link/config/ivc.h`, `system/link/ivc.h`, `link/unicast/ivc.c`
* **CUSTOM link** — the same three shapes
* **Orin SPE** — `system/platform/freertos/orin_spe.h`
* **json encoder** — `utils/json_encoder.{h,c}` + test
* plus `session/weak_session.h` and the colliding `session/keyexpr.h`

## Two approaches that were tried and do NOT work

Recorded so nobody spends the afternoon again:

1. **Graft the root onto `upstream-1.7.2` and `git rebase --onto upstream-1.8.0`.**
   This is the right idea — it gives git the merge base it lacks and lets rename
   detection work. `git replace --graft` writes a well-formed ref
   (`git replace -l` lists it, `git cat-file -p` shows the parent) but
   `rev-list --parents` does not honour it in this submodule checkout, so the
   rebase has nothing to replay. Worth one more attempt from a standalone clone
   before abandoning.
2. **Hand-rewrite the moved paths in the patch and `git apply -3`.** Loses
   rename detection, which is what broke on keyexpr in the first place, and
   `apply` is atomic so one bad file reverts all 138.

## Work items

**W1 — resolve the keyexpr name collision.** A decision, before any merging.
Rename ours (`session/nros_keyexpr.h`? it is ours, so it should move) and fix
its includes. Everything else is downstream of this.

**W2 — replay the line onto 1.8.0, commit by commit, NOT squashed.** The
existing 32 patches stay 32 patches; the orphan root's delta becomes one commit
because it already is one. Prefer the graft-and-rebase route from a standalone
clone so renames are detected; fall back to per-commit cherry-pick.

**W3 — build.** Native and Zephyr, through `zpico-sys`. A resolved conflict that
does not compile is not resolved.

**W4 — prove the superset before pushing**, per CLAUDE.md's force-push rule:

```
comm -23 <(git ls-tree -r --name-only backup/nano-ros-1.7.2-20260903 | sort) \
         <(git ls-tree -r --name-only port/nano-ros-1.8.0 | sort)
```

must be empty, and each of the 17 added files spot-checked for content, not just
presence.

**W5 — force-push `nano-ros`, then bump the superproject pin** in that order
(CLAUDE.md: push the fork branch FIRST). The backup branch stays as the recovery
ref.

## Acceptance

* `nano-ros` is 1.8.0 plus our commits, individually preserved.
* `git diff 1.8.0..nano-ros` is a meaningful question with a readable answer —
  which it is not today, because the line has no upstream ancestry at all.
* The zpico build passes on native and Zephyr.
* The tree-superset check above is empty.

## Recovery

`backup/nano-ros-1.7.2-20260903` -> `fa7ad0f5b`, pushed and verified on the
remote 2026-09-03 BEFORE any porting began. If W5 goes wrong, that ref is the
1.7.2 line intact.
