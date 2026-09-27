---
id: 1509
title: "A node package names no platform and no RMW — measured true today, gated
  by nothing; the end-to-end lane that would notice is `schedule`-only"
status: open
type: enhancement
area: testing, build
severity: medium
found: 2026-09-27
related: [issue-1453, issue-1429, issue-1108, issue-1226, issue-1040, issue-0196, issue-0743, issue-1043, issue-1289]
---

## The invariant, stated so it can be checked

A workspace has three kinds of package, and only one of them is allowed to know
what it is running on:

* the **node package** — application logic. It publishes, subscribes, serves,
  and declares its entities. It must look the **same** whatever platform the
  image targets and whatever RMW carries its messages.
* the **entry package** (`*_entry`) — the platform seam. `extern crate zephyr;`,
  `zephyr_build::export_kconfig_bool_options()`,
  `nros_zephyr_build::bake_nros_config()` live here and belong here.
* the **bringup** — the declaration. `system.toml` (`rmw`, `[[domain]]`,
  `[image.*]`, `[system].features`) and the launch files say which backend,
  which board, which topology.

This is not aspirational. It was **measured on 2026-09-27 and it holds**:
across `examples/workspaces` and the six buildable `examples/templates`, **126
node-package source files, zero of which name a platform or an RMW in CODE.**
Every code-level hit in those trees is inside an `*_entry` package.

The value of that is concrete. It is why a node package can be lifted from the
native workspace into the Zephyr one by changing a bringup row; it is why
`examples/workspaces/safety` can carry C, C++ and Rust side by side against one
`features = ["safety"]` line; and it is the property that makes
`examples/templates` copy-out-able at all — a stranger who copies a node package
is copying application logic, not a decision about their transport.

**Nothing in the tree asks whether it still holds.** The invariant is currently
maintained by the fact that everybody who has touched these trees happened to
believe in it.

## Why this is filed now

It had already slipped, in the one form a build cannot catch: **comments.** A
2026-09-27 sweep found four classes of platform/RMW mention in node packages.
Two are legitimate (the entry packages themselves; the `::setvbuf` comments in
`examples/workspaces/cpp/src/{talker,listener,service_server,action_server}_pkg`
and `mixed/src/cpp_listener_pkg`, which explain why the portable spelling is
mandatory under Zephyr picolibc and are load-bearing — delete one and the next
reader "simplifies" it back and breaks Zephyr silently). Two were leaks and were
fixed in the PR that files this issue:

* **backend behaviour documented inside node packages** — seven files in all:
  five under `safety/` plus the two `bridge-*/` talkers, each explaining what
  *the zenoh backend* does. The safety nodes' behaviour is governed by `[system].features =
  ["safety"]`, not by which RMW the image picks; it is merely that only zenoh
  implements the CRC path today. That explanation moved to the bringups'
  `system.toml` and the workspace READMEs, beside the declaration that causes
  it;
* **run instructions in a module doc** —
  `examples/templates/local-msg-package/src/rust_consumer/src/main.rs` carried
  `//! Run (zenoh router must be up): … ros2 run rmw_zenoh_cpp rmw_zenohd`. This
  one actively misled: `nros new --workspace` defaults to `--rmw cyclonedds`
  (`packages/cli/nros-cli-core/src/cmd/new.rs:460`), which needs no router at
  all. It moved to that template's README and now says which RMW it assumes and
  what changes if you change the `rmw` line.

One of the moved comments was also **wrong**: `bridge-xrce`'s talker doc
described the forwarding as `zenoh→cyclonedds`, copied from its cyclonedds
sibling, in the workspace whose whole subject is that it is *not* cyclonedds. A
fact restated where it is not declared drifts from the declaration, and nothing
compares them. That is the second argument for keeping the fact in one place.

So: the invariant is true, it was silently eroding at the edges, and the repair
is cheap only while the tree is still clean. This issue's job is to keep it
holding on every PR.

## Why a NEW gate rather than an existing one

`check-template-copy-out` is the gate whose subject is closest — it copies each
template out of the tracked file set and builds it. It caught this class
**nowhere**, for two independent reasons:

1. It runs on `schedule` / `workflow_dispatch` only (issue 1453), so no
   `pull_request` and no `merge_group` event asks it anything. An unreachable
   gate protects nobody.
2. Even reachable, it would not catch this. A node package that says `zenoh` in
   a comment builds fine. A node package that `use`s a backend crate builds fine
   *in the workspace that has it*. "Does it build here" and "is it invariant" are
   different questions, and only the second one is this issue's.

Issue 1453 is explicit that merge-gating the copy-out lane costs tens of minutes
plus provisioning per batch, and takes no recommendation. **This gate does not
inherit that cost.** It reads tracked files and answers in milliseconds. It
builds nothing, resolves no fixture stamp and needs no SDK, so
`check-lane-contracts` permits it on the fast line — the rule there is that a
gate in an affordability tier may only resolve artifacts the job itself builds,
and this one resolves none.

That is the same division of labour PR #1151 struck for issue 1429: the
end-to-end lane keeps the categories no predicate can express, and the
checkable half moves to a static rule that is merge-gating and free.

## The four rules

### Rule 1 — no platform/RMW token in node-package CODE

Reject `zenoh`, `cyclonedds`, `xrce`, `zephyr`, `freertos`, `nuttx`, `threadx`,
`esp32`, `picolibc`, `native_sim`, `mps2`, `stm32`, `smoltcp` (and the obvious
siblings) appearing in a node package's source.

**It must strip comments first.** This is the requirement that decides whether
the gate is usable, and the margin is not small. Measured over the same 126
files, after the comment relocation above:

| what the rule reads | files it would fail |
| --- | --- |
| code only (comments stripped) | **0** |
| raw file text | **12** |

The twelve are all legitimate: the five `::setvbuf` files named above, four
`Listener.c` / `QosListener.c` files whose comments name Cyclone only to record
the investigation that justifies a diagnostic on the reject path ("Cyclone's own
trace said `take: returning 1` while the only observable was an absence of
output"), `mixed/src/rust_heartbeat_pkg/src/lib.rs`
(explains which targets it is `no_std` for), and
`realtime-cpp/src/aux_pkg/{include/aux_pkg/Aux.hpp,src/Aux.cpp}` (explain which
FreeRTOS tier the node is bound to). A raw-text rule fails all twelve on day
one, which is not a slow start — it is a gate that gets a blanket baseline on
its first commit and never says anything again.

Three of those twelve are worth a second look by whoever implements this, and
are deliberately **not** pre-judged here: `rust_heartbeat_pkg` and the two
`aux_pkg` files name a *platform* rather than a backend, in a comment, in a node
package. They are outside the class this PR repaired (which was about RMW
backends) and they may be the `setvbuf` case again — a portability fact whose
reason must be readable at the call site — or they may be a fourth leak. Decide
per file, with a reason, and record it.

### Rule 2 — every `<depend>` resolves

Issue 1108 is the precedent: two `package.xml` files in the templates declared
`<depend>nano-ros</depend>` — not even a legal ROS package name — and two static
gates plus a colcon-parity job were green over it for the template's whole life.

**Reuse `orchestration::prereq_resolve::classify`**
(`packages/cli/nros-cli-core/src/orchestration/prereq_resolve.rs:133`). It is the
tree's one ladder: workspace package → generated message → `[prereq.*]` key →
ROS package → self-buildtool → rosdep snapshot → `UNKNOWN`. A second spelling of
that ladder would be a second answer to one question, which is the
`fixtures.toml` `row_coord()` class (67 rows in no lane at all) and the
`check-rmw-api-parity` class (two green tools disagreeing by 25 symbols).

Note what this costs: the `ros` rung reads `AMENT_PREFIX_PATH`, so on a host with
no ROS the ladder cannot distinguish "a real ROS package" from `UNKNOWN`. That is
not a reason to skip the rule — it is the rule's **NOT VERIFIED** outcome. See
Acceptance.

### Rule 3 — shape

* **no root build file.** RFC-0098 D9 / phase-445 W5 removed the umbrella
  `CMakeLists.txt`; a workspace that grows one back has re-acquired the one
  nano-ros-specific file that made it not-a-colcon-workspace.
* **no committed `generated/`.** Those trees are codegen'd from the *user's* msg
  packages and do not exist in a fresh clone.
* **`.colcon_workspace` present.**
* **lockfile rule**, exactly `check-leaf-lockfiles`' invariant: tracked lock ⟺
  (no message deps) ∨ (committed `generated/`). Do not restate it; read it from
  wherever that gate holds it, for the same reason as rule 2.

### Rule 4 — tracked-set completeness

Every path a `CMakeLists.txt`, `package.xml` or `system.toml` references must be
in `git ls-files`. This is the **static analogue of copying out**: the copy-out
lane's real discriminating power is that it copies the TRACKED set and then
builds, so a file that exists only in the author's worktree is what it catches.
Asking "is every referenced path tracked" recovers most of that at no build cost.

It does not recover all of it — a tracked file with wrong content still builds
nowhere, which is issue 1453's residual and stays with the copy-out lane. Say so
in the gate's own header, so nobody reads rule 4 as "copy-out is covered now".

## Scope — recorded as a decision, not left to the implementation

**In scope:** node packages of bringup-shaped workspaces, under BOTH
`examples/templates` and `examples/workspaces`.

"Bringup-shaped" already has a computed spelling and the gate should use it
rather than invent one: `scripts/check-template-copy-out.sh --list` selects on
*a tracked `system.toml` declares an `[image.*]`*, and it selects exactly the six
templates (`c-and-cpp-mixed-workspace`, `local-msg-package`,
`multi-node-workspace`, `multi-node-workspace-cpp`, `multi-package-workspace`,
`pure-c-workspace`). Note two of the six carry the `system.toml` beside a
package rather than in a `demo_bringup/` — Form-1 self-bringup — so a rule
keyed on a directory NAMED `*bringup*` finds four, not six. Ask the predicate,
not the directory name.

**Exempt, each for its own reason:**

* **`*_entry` packages.** The platform seam belongs there; that is the whole
  design. The scope roots hold 126 packages: 14 entries, 21 bringups and **91
  node packages**, which is the set rules 1–4 apply to. (The 126 node-package
  SOURCE FILES counted above is a different 126 — coincidence, not a
  cross-check.)
* **`zephyr-byo`.** A platform-specific template by design — it exists to show
  bring-your-own-Zephyr integration. It has no `[image.*]` and is already in the
  copy-out lane's skipped set, but exempt it by NAME too, so that giving it an
  image later does not silently pull it in.
* **the per-platform standalone examples** — `examples/{native,zephyr,
  esp32-c3-baremetal,…}`. Their whole purpose is showing platform integration.
  Measured: `examples/zephyr` is **36 of 36** source files naming a platform or
  RMW, `examples/esp32-c3-baremetal` 4 of 4, `examples/native` 41 of 56. Putting
  these in scope would not be a strict gate, it would be a category error.

## Two implementation requirements, and the reason each is not optional

1. **Rule 1 reads code only.** Measured above: 0 failures vs 12. This is not a
   tuning parameter.
2. **Exceptions live in `.config/<gate>-baseline.txt`, one per line, each with
   its reason, RATCHETED to shrink** — the `dist-or-reason` shape. Concretely:
   the file fails when a listed path loses its violation (remove the line) *and*
   when an unlisted path gains one, so the debt can only shrink and a fixed
   entry cannot go stale. That second direction is the issue-0743 class and it
   is the half that gets skipped; `.config/dist-or-reason-baseline.txt` and
   `.config/gate-selftest-baseline.txt` both spell the header to copy.

A third, from this neighbourhood's own history: **make the gate's REACH equal
the rule it enforces.** The 2026-07-28 audit found four gates whose coverage was
narrower than their rule; `check-c-array-pool-floors` reported 21 arrays over a
tree with 23 because it required two `#define`s on adjacent lines; issue 1226 is
a gate that worked perfectly and ran nowhere. If rule 1 reads `*.rs`, `*.c`,
`*.cpp`, `*.hpp` — say so, and say what it does with a `build.rs` in a node
package (there should not be one; a node package that needs a build script is
usually an entry package in disguise).

## Acceptance

* **Green on landing.** It describes what is already true — 0 code-level
  violations over 126 files — so a red on the first run means the rule is wrong,
  not the tree.
* **Red under a mutation** that puts an RMW token into a node package's CODE:
  add `use nros_rmw_zenoh as _;` to a node package's `lib.rs` and the gate must
  name that file and that token. A mutation that puts the same token in a
  COMMENT must stay green — both directions, or rule 1's stripping is untested.
* **A selftest on the normal path**, so the gate appears in neither
  `.config/gate-selftest-baseline.txt` nor `.config/ungated-gates.txt`. A gate
  with no negative control prints the same thing as a gate that cannot fail —
  which is what `check-reconfigure-stale` needed its own negative control for.
* **Three outcomes, not two**, per issue 1043: **OK**; **FAIL** (a violation was
  measured); **NOT VERIFIED** — reported through the `nros_check_skip` ledger
  (`scripts/build/check-skip.sh`), which is what rule 2 must return on a host
  with no `AMENT_PREFIX_PATH`, because there the classifier cannot tell a real
  ROS package from an unknown name. Exiting 0 there would make `just check`
  print "All checks passed!" about a question nobody asked.
* **Registered so it RUNS.** `just check <name>` plus membership in the
  `ci gate` `steps=(…)` array — `check-default-gates-run-somewhere` (issue 1040)
  reads both since issue 1226 widened its scope past `just check` names. Add the
  name to `.config/gate-registry-baseline.txt`, which `check-gate-lists`
  requires (issue 1071: a PR deleted four gates and every gate stayed green,
  because sorted-and-one-per-line is a property a deletion satisfies).
* **Cost stated, measured.** It should be milliseconds. If it is not, the
  argument in "Why a NEW gate" does not hold and the placement has to be
  re-decided.

## What this issue does NOT claim

It does not claim the copy-out class is covered — issue 1453's residual (a
template whose tracked content is complete and still does not build) is
untouched by every rule here, and that issue's cost decision is still open.

It does not claim node packages are backend-agnostic in every respect. They
still carry `[[package.metadata.nros.node.publishes]]` entity rows in their
manifests, which RFC-0098 D5 moves to the bringup's `[[component]]` row and has
not yet moved — that is issue 1289, and it is a different axis (declaration
location) from this one (platform/RMW invariance).
