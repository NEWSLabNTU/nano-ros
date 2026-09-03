# Phase 415 — the zenoh-pico patch line moves to 1.8.0

**Status (2026-09-04). DONE.** `nano-ros` is 1.8.0 + 65 of our commits,
individually preserved; the superproject pin moves with it.

**The 2026-09-03 survey below was WRONG, and the correction is the useful part
of this doc.** It reported an orphan line with no upstream ancestry, a keyexpr
name collision needing a decision, 42 conflicting files, and a 6,448-insertion
delta over 138 files. None of that held:

| the survey said | measured 2026-09-04 |
| --- | --- |
| no upstream ancestry; graft needed | `1.7.2` **is** an ancestor of `nano-ros` |
| our delta = 138 files / 6,448 ins | **69 files / 7,252 ins** above the true fork point |
| `session/keyexpr.h` collides — a DECISION | no collision; that file is **upstream's**, inherited |
| admin space, json encoder are OURS | both **upstream's** (`#1125`, inherited) |
| a merge, not a replay | a plain `git rebase --onto`, 64 commits, 8 conflicted |

**Root cause of the bad survey: it diffed against the `1.7.2` TAG, but the
branch forks from upstream `main` EIGHT commits past that tag** — at
`2bd54691 Refcount clean up (#1146)`. Those eight are upstream's own work
(including `ea1ecbe9`, the keyexpr move, and `1d9e198f`, the admin space), so
diffing from the tag attributed them to us and invented the collision. The
fork point is also an **ancestor of 1.8.0**, which is what makes the whole
thing a rebase.

The lesson generalises, and CLAUDE.md already carries half of it: *resolve a
fork's relationship by containment, not by guessing.* One command would have
saved the afternoon:

```bash
git merge-base <branch> upstream/main     # the fork point
git merge-base --is-ancestor <fork-point> <target-tag> && echo "plain rebase"
```

The survey was probably run against the SUBMODULE checkout, which is
single-branch and shallow — the exact condition CLAUDE.md warns makes a fork's
history read as something it is not.

## What actually happened

```
git rebase --onto 1.8.0 2bd54691 nano-ros    # 64 commits, 8 needed a hand
```

* **1 commit dropped as obsolete**: `63100545` (Race 1, `_z_get_query_id` under
  the session mutex). Upstream fixed it independently and better — 1.8.0 folds
  the id allocation into `_z_unsafe_register_pending_query`, called under the
  mutex, so the separate helper our commit added has no reason to exist.
* **2 commits added**, both fixing things the port exposed (see below).
* Everything else replayed with its message intact.

### Two defects the port exposed

**1. Upstream 1.8.0 does not compile with `Z_FEATURE_MATCHING=0`.**
`_z_write_filter_clear` is unguarded but calls
`_z_write_filter_ctx_remove_callbacks`, declared and defined only under
`#if Z_FEATURE_MATCHING`. That is exactly nano-ros's Zephyr configuration, so
every Zephyr zenoh image failed to build. Fixed by guarding the call — correct
rather than expedient, because the state it clears
(`_z_write_filter_ctx_t::callbacks`) is guarded the same way. **Worth reporting
upstream.**

**2. Our own knobs were in the generated file, not its template.**
`CMakeLists.txt` runs `configure_file(config.h.in -> config.h)` into the SOURCE
tree, so `Z_FEATURE_TX_SPLIT_LOCK` and `Z_TRANSPORT_LEASE_TASK_SLEEP_CHUNK_MS`
— written only into `config.h` — were erased by the first cmake configure. This
was equally true at 1.7.2 and never bit, because `zpico-sys` compiles the
sources directly and never runs that CMakeLists. Both now live in `config.h.in`.

**Still outstanding, deliberately not fixed here:** the whole `#ifndef` wrapping
from `49012370 fix: allow feature macro overrides` is also generated-file-only —
regenerating `config.h` drops 118 lines of it. Pre-existing, not a port
regression, and not a build blocker, so it stays a known gap rather than scope
creep. Sweep:

```bash
comm -23 <(grep -oP '^#define \K[A-Z_0-9]+' include/zenoh-pico/config.h    | sort -u) \
         <(grep -oP '^#define \K[A-Z_0-9]+' include/zenoh-pico/config.h.in | sort -u)
```

### The one design divergence from upstream

Both lines now carry a `_mutex_transport` on the session, but they use it
differently, and ours is deliberate:

* **Upstream 1.8.0** holds the lock across `_z_transport_clear` in
  `_zp_unicast_failed` — and does **not** take it in `_z_send_n_msg` /
  `_z_send_n_batch`. A mutex only excludes when both sides take it, so the
  publisher still races the free. Upstream added the writer half only.
* **Ours (issue 0899)** is a HANDSHAKE: publish `_tp._type = _Z_TRANSPORT_NONE`
  under the lock, release, then tear down without it — because holding it
  across `_z_link_free` **deadlocks** in lwIP (measured; the lease task parks
  and the image goes quiet instead of asserting). Trading a crash for a hang is
  not a fix. The publisher half is ours and is what actually closes the race.

Our call sites were renamed to upstream's spelling
(`_z_session_transport_mutex_lock/_unlock`) so there is one vocabulary.
Issue 0924's `_reconnecting` claim rides on the same lock.

## Verification

* `git rebase --onto` replayed 64 commits; 65 on the branch after the two adds.
* **Tree superset vs the backup**: the only path present on the old line and
  absent on the new is `src/collections/seqnumber.c`, which **upstream deleted**
  in `67fe16c0 Add generic atomics (#1170)`. No patch of ours was lost; all 27
  files our line ADDS are present, spot-checked byte-identical.
* **Native**: zenoh-pico's own cmake build, twice — once with the regenerated
  `config.h`, once with our committed one. Both clean. All four of our link
  families compile (isotp/can/ivc/custom, 12 objects).
* **`zpico-sys`**: builds against 1.8.0 — the real nano-ros path.
* **Zephyr**: `just zephyr build-rust-examples` green. (The two
  `zephyr_self_pkg_*` `FATAL ERROR` lines in that log are expected: those are
  `west-configure` fixtures whose gate is the `output` artifact written before
  the configure gives up.)

## Recovery

`backup/nano-ros-1.7.2-20260903` -> `fa7ad0f5b`, verified on the remote both
before the port and after the force-push. If anything here proves wrong, that
ref is the 1.7.2 line intact.

---

# Original survey, 2026-09-03 (superseded — kept for the record)

**Its conclusions are wrong; see the correction above.**

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
