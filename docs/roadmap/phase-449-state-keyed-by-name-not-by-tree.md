# Phase 449 — build state keyed by a NAME, not by the tree that asked for it

**Status (2026-09-11). Opened to give eight homeless issues one owner. Nothing
in this phase has landed; W1–W8 are open. Every member issue is open and was
filed from a measured incident, not from a reading.**

## Why this phase exists

Eight open issues report the same defect at eight different layers. In each of
them a piece of build state — a lock, a build root, a skip ledger, an SDK tree,
a west workspace, a cmake package registry entry — is addressed by a NAME or by
an absolute path baked at provisioning time, and not by the checkout that asked
for it. A second checkout on the same host then silently takes part in this
one's build.

None of them was found by reading the code. Each was found when the wrong
artifact shipped or the wrong verdict printed, and in every case **the message
named something else**:

* `subtree-guard` refused a build saying *"two builds in one tree"* when the
  other build was in an unrelated clone ([#1157](../issues/1157-subtree-guard-lock-is-host-global-not-per-tree.md)).
* A `just check fast` summary listed skips that were untrue of the checkout that
  printed them — the linked worktree's, delivered into the parent's ledger
  ([#1234](../issues/1234-worktree-agents-share-the-parent-build-dir.md)).
* `just nuttx build-riscv-c`, run inside a worktree, reconfigured the MAIN
  checkout's NuttX kernel from `qemu-armv7a` to `rv-virt` and rewrote its
  `.config`, `include/nuttx/config.h` and `nuttx` binary. Nothing in the
  worktree's own `third-party/nuttx` was touched and nothing said so
  ([#1280](../issues/1280-worktree-inherits-foreign-sdk-paths.md)).
* Tier 2 produced **no verdict for eight consecutive scheduled runs**, and the
  same SHA failed two different ways on two different days, because the Zephyr
  workspace it built in belongs to a second checkout
  ([#1253](../issues/1253-zephyr-workspace-manifest-binds-foreign-module.md)).

That last one is the cost that orders this phase. A lane whose outcome depends
on something outside the commit under test has no signal capacity at all, which
is the failure CLAUDE.md already names for a uniformly-red lane — here arrived
at from the other direction.

## The shape, stated once

**A build artifact must be a function of the tree that produced it.** Anything
shared between checkouts must be keyed by the tree (git common dir, repo root,
`NROS_BUILD_ROOT`), or be provably content-addressed, or be refused. A name is
not a key: `<name>.pgid`, `nano-ros`, `build`, `zephyr-workspace` are all names
that two clones on one host collide on.

Two adjacent facts this phase must not re-learn:

* **Insulation in one place is not insulation.** #1280's incident cost no ARM
  build, because ARM images link against a per-architecture NuttX export rather
  than the shared configured tree. That insulation is specific to the NuttX
  kernel and answers neither of the harms the issue records.
* **`NROS_ZEPHYR_WORKSPACE`, `NROS_REPO_DIR` and the `sdk-env.just` paths are
  inherited through the ENVIRONMENT**, so they defeat a per-tree default without
  appearing in any command line. #1166's runner pins the first in
  `actions-runner/.env`; #1234's worktree inherits the second from its parent
  shell; #1280 counts **19** absolute paths in the third.

## Work items

Ordered by consequence: the items where a WRONG ARTIFACT ships silently come
before the items where a wrong VERDICT prints, and the repair gap comes last.

### W1 — a Zephyr workspace names the checkout whose module it builds

[Issue 1258](../issues/1258-zephyr-store-workspace-is-bound-to-one-checkout.md).
`scripts/zephyr/setup.sh` replaces the west manifest project with a symlink to
the checkout that ran it, so the `nros` Zephyr module of EVERY build in that
workspace comes from that one tree, measured in a downstream project's
`build/zephyr_modules.txt`.

- [ ] A workspace either resolves `nros` from the checkout invoking the build,
      or refuses and says which checkout it is bound to.
- [ ] A downstream project's `zephyr_modules.txt` names its own pinned
      submodule, shown on a real board build.

### W2 — tier 2 builds the tree under test

[Issue 1253](../issues/1253-zephyr-workspace-manifest-binds-foreign-module.md).
W1's defect as the lane experiences it: the runner's workspace lives inside a
second nano-ros checkout, so tier 2 compiled that tree's headers beside this
tree's entries and judged images no run of this tree made.

- [ ] The lane fails LOUDLY when its workspace belongs to another checkout,
      rather than building and reporting an unattributable failure.
- [ ] One scheduled tier-2 run reaches the cells and produces a verdict. This
      is the acceptance; the three earlier failure texts are not.

### W3 — the Zephyr SDK lives in the store, not in a clone

[Issue 1254](../issues/1254-zephyr-sdk-installs-inside-the-checkout.md).
Provisioning installs to `<clone>/scripts/zephyr/sdk/`, and a downstream
project's generated `env.sh` then exports a `ZEPHYR_SDK_INSTALL_DIR` pointing
into a sibling clone. Every board build of that project has depended on that
clone existing and on nobody running `just clean-setup` in it.

- [ ] The SDK is provisioned under the store, per RFC-0095 D2 and the rung
      [phase-447](phase-447-provisioning-revision.md) A1/A2 add.
- [ ] A generated `env.sh` contains no path inside any nano-ros checkout.

### W4 — a worktree build uses the worktree's SDK trees

[Issue 1280](../issues/1280-worktree-inherits-foreign-sdk-paths.md). 19
`sdk-env.just` paths are absolute and inherited, and they win over the worktree
that set out to build.

- [ ] Each of the 19 is either derived from the invoking tree or declared
      shared-on-purpose with a reason.
- [ ] A worktree build touches no file outside the worktree — checked by
      comparing the parent checkout byte for byte across one build, the method
      `check-hook-repo-side-effects` already uses.

### W5 — a worktree's build dir and skip ledger are its own

[Issue 1234](../issues/1234-worktree-agents-share-the-parent-build-dir.md). A
linked worktree inherits `NROS_REPO_DIR`, so it writes its build dir and its
skip ledger into the parent, and one tree's skips appear in another's gate
summary. Parallel agent sessions are the normal way work happens here, so this
is not a rare configuration.

- [ ] `NROS_REPO_DIR` resolves per tree (git common dir, not the inherited
      value), or the inheritance is refused with the two paths named.
- [ ] Two `just check fast` runs in two trees report only their own skips.

### W6 — the build guard is keyed by tree

[Issue 1157](../issues/1157-subtree-guard-lock-is-host-global-not-per-tree.md).
`/tmp/nros-build-guards/<name>.pgid` is keyed by build NAME and by nothing else,
so an unrelated checkout blocks this one's gates while the refusal says the
opposite.

- [ ] The lock path carries the tree.
- [ ] The refusal text names the tree holding the lock, so a wrong one is
      diagnosable from the message alone.

**#1157 and #1166 are NOT duplicates and must not be merged** — #1166 says so
itself. They are opposite halves: one refuses across trees that should be
independent, the other shares across trees that should be.

### W7 — the runner builds into its own checkout

[Issue 1166](../issues/1166-ci-runner-builds-into-a-developer-checkout.md). The
self-hosted runner's `.env` pins `NROS_ZEPHYR_WORKSPACE` into a developer's
working tree, the lane's own verification reports that as `[OK]`, and a west
build dir carries neither the board nor the checkout in its name.

- [ ] The runner's workspace is its own.
- [ ] The verification that printed `[OK]` fails on the configuration it
      passed. A check that could not have failed is the subject of
      [phase-450](phase-450-gate-reach-narrower-than-its-rule.md); this item
      owes it one worked case.

### W8 — `just setup zephyr` can repair what it skips

[Issue 1279](../issues/1279-zephyr-setup-skips-sdk-registration-when-workspace-exists.md).
The verb skips the whole SDK setup when the WORKSPACE directory exists, but the
cmake package registry entry is per-USER state. A host can hold a complete,
unpacked, unusable SDK that the verb cannot repair.

- [ ] The skip is per-artifact, not per-workspace: registration is checked and
      repaired independently of whether the workspace is present.
- [ ] Removing the registry entry and re-running the verb restores it, without
      `--force` and without a re-download.

## Acceptance for the phase

* A second checkout of nano-ros on the same host changes no artifact, no lock
  and no verdict of the first, demonstrated by running one build and one gate
  lane in each concurrently.
* Tier 2 produces a runtime verdict.

## Non-goals

* Making SDK trees per-tree *copies*. Sharing a 2.5 GB SDK between checkouts is
  correct; being unable to say which tree a build used is the defect.
* The provisioning ladder itself — that is
  [phase-447](phase-447-provisioning-revision.md). W3 states where the SDK must
  land and lets 447 own how it gets there.
* Fixing the tier-2 lane's other stages. Issue 1158's "no runtime verdict in six
  scheduled runs" is homed at
  [phase-416](phase-416-tier2-lane-and-single-spelling.md); W2 here removes one
  of its causes and does not claim the rest.
