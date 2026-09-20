# Phase 449 — build state keyed by a NAME, not by the tree that asked for it

**Status (2026-09-18). W4 and W5 are DONE; W1–W3 and W6–W8 are open. Every
premise below was RE-MEASURED against the tree on 2026-09-18 rather than
re-read, because a phase doc is prose and prose does not re-derive on a
rebase — this campaign has already retracted two conclusions that went stale
exactly that way.**

**W5 closed without anyone touching this file.** `NROS_REPO_DIR` is
`justfile_directory()` in `just/sdk-env.just` now, not `env(...)`, and
`nros_build_root()` re-roots both REPO rungs through
`nros_reroot_checkout_path` — landed by W4's rule plus phase-454 W3. Measured,
not inferred: a linked worktree carrying the MAIN checkout's `NROS_REPO_DIR`
resolves its build root and its `check-skips` ledger to its own tree. Issue
1234's defect does not reproduce.

**One PRESCRIPTION here is superseded, which matters more than one item
closing.** This phase says shared state must be keyed by "git common dir"; a
linked worktree's `.git` is a FILE, not a directory
([#1336](../issues/archived/1336-cli-source-stamp-unwatched-in-worktree.md)), so
`scripts/lib/checkout-paths.sh` deliberately uses a MARKER WALK and says so at
its own § "Why the marker and not `.git`". Anything W6 keys by tree must reach
for that marker, not for the mechanism this document named.

**The five open premises all still hold, verbatim**, and W2's has not moved at
all: `just matrix-triage` on 2026-09-18 reports **0 of 8 runs reached the
cells**, the same count this phase opened on a week earlier — 4 stopped in
provisioning, 3 in the build. The lane still has no signal capacity.

Opened to give eight homeless issues one owner. Every one of their issues was
filed from a measured incident, not from a reading.

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
  other build was in an unrelated clone ([#1157](../issues/archived/1157-subtree-guard-lock-is-host-global-not-per-tree.md)).
* A `just check fast` summary listed skips that were untrue of the checkout that
  printed them — the linked worktree's, delivered into the parent's ledger
  ([#1234](../issues/1234-worktree-agents-share-the-parent-build-dir.md)).
* `just nuttx build-riscv-c`, run inside a worktree, reconfigured the MAIN
  checkout's NuttX kernel from `qemu-armv7a` to `rv-virt` and rewrote its
  `.config`, `include/nuttx/config.h` and `nuttx` binary. Nothing in the
  worktree's own `third-party/nuttx` was touched and nothing said so
  ([#1280](../issues/archived/1280-worktree-inherits-foreign-sdk-paths.md),
  resolved 2026-09-12).
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
shared between checkouts must be keyed by the tree, or be provably
content-addressed, or be refused. A name is not a key: `<name>.pgid`,
`nano-ros`, `build`, `zephyr-workspace` are all names that two clones on one
host collide on.

**"Keyed by the tree" has one spelling, and this document first named the wrong
one.** It said "git common dir"; a linked worktree's `.git` is a FILE
([#1336](../issues/archived/1336-cli-source-stamp-unwatched-in-worktree.md)), so the
answer is the CHECKOUT MARKER walk that `scripts/lib/checkout-paths.sh`
implements and `nros_launcher::checkout::MONOREPO_MARKER` mirrors — the same
resolver W4 already landed. A second spelling here would be the defect this
phase is about, one layer up.

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

[Issue 1258](../issues/archived/1258-zephyr-store-workspace-is-bound-to-one-checkout.md).
`scripts/zephyr/setup.sh` replaces the west manifest project with a symlink to
the checkout that ran it, so the `nros` Zephyr module of EVERY build in that
workspace comes from that one tree, measured in a downstream project's
`build/zephyr_modules.txt`.

- [x] A workspace either resolves `nros` from the checkout invoking the build,
      or refuses and says which checkout it is bound to. **Both halves.**
      `setup.sh` no longer ends with `ln -sf "$NANO_ROS_ROOT"`: the manifest
      project is a real directory holding the manifest FILE and nothing else,
      so it carries no `zephyr/module.yml` and contributes no module. Each
      build names its own with `-DZEPHYR_EXTRA_MODULES=<checkout>` — the module
      ROOT, not `<checkout>/zephyr`, because a zephyr module is the directory
      holding `zephyr/module.yml` and the subdirectory fails configure with
      *"is not a valid zephyr module"*, a message that names the variable and
      not the rule. Three `west build` sites carry it:
      `zephyr-fixture-run-one.sh`, `check-copy-out.sh`, `tests/zephyr/run-c.sh`.
      The refusal half is `check-zephyr-workspace-checkout.sh`, which W2 wires
      into the lane that needed it.
- [x] A `zephyr_modules.txt` names the invoking checkout, shown on a REAL
      board build — the acceptance as written asks for a downstream project,
      and this is the same fact with the same mechanism, measured here because
      no downstream tree is available to this checkout.
      Against an isolated workspace in the migrated shape (manifest-only
      project, no symlink), `west build -b native_sim/native/64` on
      `examples/zephyr/rust/talker` configures, compiles and reaches
      `Linking C executable zephyr/zephyr.elf`, with
      `"nros":"/home/aeon/repos/nano-ros"` — where the live bound workspace
      gives `"nros":"<workspace>/nano-ros"`.
- [x] **Migration is transparent, which was the risk worth measuring.** A
      workspace provisioned BEFORE this still carries the symlink, and the
      builders now also pass the module — so Zephyr sees `nros` twice. Measured
      on the live bound workspace: no duplicate-module error, configure
      proceeds, and the resolved module is the CHECKOUT's. Where the two would
      disagree — a workspace bound to a FOREIGN checkout — W2's guard refuses
      before the build. `setup.sh` also unbinds an existing workspace in place
      (`unbind-manifest-project.sh`, idempotent) rather than asking anyone to
      re-provision 4.6 GB.

### W2 — tier 2 builds the tree under test

[Issue 1253](../issues/1253-zephyr-workspace-manifest-binds-foreign-module.md).
W1's defect as the lane experiences it: the runner's workspace lives inside a
second nano-ros checkout, so tier 2 compiled that tree's headers beside this
tree's entries and judged images no run of this tree made.

- [x] The lane fails LOUDLY when its workspace belongs to another checkout.
      **The gate already existed and the lane did not run it** —
      `check-zephyr-workspace-checkout.sh`, written for this very issue, whose
      header names the two tier-2 nights (2026-09-06, -07) it was written
      after. It was reached only through `check-tier-preconditions.sh`, which
      only the TIER-1 `ci` recipe runs. So the gate written FOR this lane was
      never run BY it: phase-450's subject, arriving inside this phase.
      Wired into `_matrix-run` and `matrix-nightly`, immediately after
      `_lane-gate` and BEFORE the build — issue 1253's complaint is not that
      the refusal is missing but that it arrives fifteen minutes in, inside a
      cmake configure, naming a binary rather than the workspace. Measured: on
      a workspace bound to a foreign checkout it exits 1 and prints the
      workspace, that checkout and this tree.
- [ ] One scheduled tier-2 run reaches the cells and produces a verdict. This
      is the acceptance; the three earlier failure texts are not.
      **Still owed, and not claimable from a checkout.** A scheduled run is the
      only thing that can satisfy it. As of 2026-09-18 `just matrix-triage`
      reports 0 of 8 runs reaching the cells — 4 stopped in provisioning, 3 in
      the build — so this item stays open until a nightly says otherwise.
      W1 removes one cause; issue 1158 at
      [phase-416](phase-416-tier2-lane-and-single-spelling.md) owns the rest,
      and this item does not claim them.

### W3 — the Zephyr SDK lives in the store, not in a clone

[Issue 1254](../issues/1254-zephyr-sdk-installs-inside-the-checkout.md).
Provisioning installs to `<clone>/scripts/zephyr/sdk/`, and a downstream
project's generated `env.sh` then exports a `ZEPHYR_SDK_INSTALL_DIR` pointing
into a sibling clone. Every board build of that project has depended on that
clone existing and on nobody running `just clean-setup` in it.

- [x] The SDK is provisioned under the store. `setup.sh` no longer passes
      `--prefix` — which is the documented out-of-store escape hatch, and why
      `nros sdk-path` could not find the SDK afterwards and `nros store gc` did
      not know it existed — so `nros setup --tool` installs where RFC-0095 D2
      says.
      One resolver, `scripts/lib/zephyr-sdk.sh`, the sibling of
      `zephyr-workspace.sh`: override, then STORE, then the in-checkout copy.
      The store arm is FIRST here and last there, for the same reason — it is
      existence-checked, so a host provisioned before this keeps resolving to
      its checkout copy and nothing moves until something installs to the store.
      All three arms measured: this host (legacy copy, empty store) resolves to
      the checkout; a host with only a store copy resolves to the store; a host
      with neither gets the store as the install TARGET.
      It asks `nros sdk-path` for the store layout rather than restating
      `sdk/<tool>/<version>`, and appends the tarball's own
      `zephyr-sdk-<version>/` — the level `sdk-path` alone cannot give, and the
      one thing issue 1254 says a consumer cannot get today.
- [x] A generated `env.sh` contains no path inside any nano-ros checkout.
      `ZEPHYR_SDK_INSTALL_DIR` is the resolved SDK, which is the store once
      provisioning has run. And `NANO_ROS_ROOT` no longer names one either: it
      used to be `$WORKSPACE/nano-ros`, the symlink W1 removed, so it would now
      point at a directory holding only a manifest file. A shared workspace
      cannot name one checkout — every caller already has one (`activate.sh`
      exports it, `just` derives it) — so `env.sh` preserves what the caller set
      and says what to do when there is none, instead of inventing an answer
      that is wrong for everyone but one tree. Its build hint gained
      `-DZEPHYR_EXTRA_MODULES` for the same reason.
- [x] The seven readers that CONSTRUCTED the checkout path now ask: four in
      `just/zephyr-setup.just` (which also carried four copies of the `0.16.8`
      the resolver owns), the test harness in `nros-tests/src/zephyr.rs`, and
      `runner-doctor.sh`'s ladder — which had env -> registry -> checkout and
      no store arm at all.
      The doctor's new arm is self-tested BOTH ways: a store copy is found and
      beats a checkout copy staged beside it, and the cmake registry still
      outranks the store. The first version of that arm read the library from
      the tree under EXAMINATION rather than from the doctor, so it was
      unreachable and the case passed by falling through — caught by the
      self-test, which is what one is for.
- [x] `nros store list` shows the SDK and `nros store gc` respects it —
      OBSERVED, not inferred. `nros setup --tool zephyr-sdk` was run against
      this change:

          7.9 GiB  installed  sdk/zephyr-sdk/0.16.8          # store list
          sdk/zephyr-sdk/0.16.8  (nros-sdk-index.toml, nros-sdk.lock)   # gc

      The `nros-sdk.lock` membership is the whole point: `--prefix` is what kept
      it out, which is why `nros sdk-path` could not find the SDK and `gc` did
      not know it existed. `gc --older-than 30d --dry-run` lists it among the
      entries attributable to an install and would remove nothing.
      The installed tree is **7.9 GiB**, not the 1.3 GiB this file first guessed
      — that figure was the DOWNLOAD the old hand-rolled aria2c block fetched,
      and the unpacked SDK with its toolchains is six times it. Recorded because
      the disk cost is the argument for sharing one copy between checkouts.
      And the ladder moved, exactly as designed: with a store copy AND the
      legacy checkout copy both present, the resolver now answers with the
      store. Nothing moved until something installed there.

### W4 — a worktree build uses the worktree's SDK trees — DONE

[Issue 1280](../issues/archived/1280-worktree-inherits-foreign-sdk-paths.md),
resolved and archived on 2026-09-12 by `8eeaa05ac`. The item was written here
while the issue was open; it landed on `main` first, so this section records
what closed it rather than what is owed.

The census came out at **24, not 19** — the 19, plus `NROS_LAN9118_LWIP_DIR`,
`PX4_AUTOPILOT_DIR`, the derived `IDF_PATH`, and `NROS_REPO_DIR` /
`nano_ros_ROOT` from the activate files. That is the phase's own point made
again: an enumeration is not the mechanism, and the fix is the three-valued
rule in `scripts/lib/checkout-paths.sh` (outside any checkout → KEEP, a
DIFFERENT checkout → RE-ROOT and say so, this checkout → KEEP).

- [x] Each path-valued export is derived from the invoking tree or declared
      shared-on-purpose with a reason — `just check inherited-checkout-paths`
      (fast lane) asserts coverage over `just/sdk-env.just`, one spelling of the
      checkout marker, and the rule's BEHAVIOUR against synthetic checkouts,
      because reading the source alone would pass an implementation that never
      looks at the filesystem.
- [x] A worktree build compiles the worktree's sources — measured with an
      `#error` in the worktree's `nros-platform-freertos/src/platform.c`: 0 hits
      before (the main checkout's copy was compiled), 4 after, and still 0 both
      ways for a genuine out-of-tree `NROS_PLATFORM_FREERTOS_SRC`.

### W5 — a worktree's build dir and skip ledger are its own — DONE

[Issue 1234](../issues/1234-worktree-agents-share-the-parent-build-dir.md). A
linked worktree inherits `NROS_REPO_DIR`, so it writes its build dir and its
skip ledger into the parent, and one tree's skips appear in another's gate
summary. Parallel agent sessions are the normal way work happens here, so this
is not a rare configuration.

**Closed by other work, and this file did not notice for six days** — which is
the phase's own subject wearing a different hat: a conclusion written as prose
does not re-derive when the code beneath it moves. W4's re-root rule plus
phase-454 W3 did it; neither had reason to edit this section.

- [x] `NROS_REPO_DIR` resolves per tree. `just/sdk-env.just` sets it from
      `justfile_directory()` and NOT from `env(...)` — its comment states the
      reason this phase would have: *"there is nothing an inherited value could
      be more right about"*. `nros_build_root()` then re-roots both REPO rungs
      (`NROS_REPO_ROOT`, `NROS_REPO_DIR`) through `nros_reroot_checkout_path`,
      so an inherited root belonging to ANOTHER checkout lands back on this
      tree. `NROS_BUILD_ROOT` is deliberately left alone, because its Rust
      mirror `nros_tests::build_root` reads it unre-rooted and a writer/reader
      split would be a fresh instance of this same bug.
- [x] Measured on a real linked worktree rather than read: with
      `NROS_REPO_DIR` set to the MAIN checkout, the worktree resolves its build
      root to `<worktree>/build` and its ledger to
      `<worktree>/build/check-skips`. Issue 1234's defect does not reproduce.
      (The acceptance as first written — "two `just check fast` runs report only
      their own skips" — is the same fact at one remove; the ledger PATH is what
      decides it, and that is what was measured.)

### W6 — the build guard is keyed by tree

[Issue 1157](../issues/archived/1157-subtree-guard-lock-is-host-global-not-per-tree.md).
`/tmp/nros-build-guards/<name>.pgid` is keyed by build NAME and by nothing else,
so an unrelated checkout blocks this one's gates while the refusal says the
opposite.

- [x] The lock path carries the tree:
      `<dir>/<basename>-<cksum>/<name>.pgid`. The tree comes from
      `nros_checkout_root` — the MARKER walk, sourced rather than
      reimplemented, because a linked worktree's `.git` is a file (issue 1336)
      and a second spelling of "which checkout" is this phase's own defect one
      layer up. The tree is a DIRECTORY segment, not part of the filename, so
      `runner-sweep.sh` can still enumerate every tree's locks on a shared
      runner — which it legitimately wants to do. A caller outside any checkout
      keys on its cwd: not the old behaviour restored, but the honest answer
      when there is no checkout to name.
- [x] The refusal names both trees — `Holding tree:` and `This tree:` — from a
      third field in the lock (`<launcher> <pgid> <tree>`), appended so the two
      existing `awk` readers are untouched.
- [x] Measured on two real trees, both directions, with a negative control.
      A live `fixtures` lock held by the main checkout does NOT block a linked
      worktree (`rc=0`), and DOES block the main checkout (`rc=1`) with both
      paths printed. The control: the old one-line spelling
      (`<dir>/<name>.pgid`) evaluated in each tree returns the SAME path, which
      is issue 1157 exactly — so the test distinguishes the fix from its
      absence rather than passing either way.
      `runner-sweep.sh` gained `nros_guard_reap_lock <lockfile>`: a bare name no
      longer identifies which tree's build is being reaped, and recomputing the
      path from the sweeper's own cwd would reap a different tree's lock. Its
      glob keeps the legacy `*.pgid` form beside `*/*.pgid`, so a lock written
      before this change is still swept rather than orphaned forever by the fix
      that tidied it.

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

[Issue 1279](../issues/archived/1279-zephyr-setup-skips-sdk-registration-when-workspace-exists.md).
The verb skips the whole SDK setup when the WORKSPACE directory exists, but the
cmake package registry entry is per-USER state. A host can hold a complete,
unpacked, unusable SDK that the verb cannot repair.

- [x] The skip is per-artifact, not per-workspace.
      `scripts/zephyr/ensure-sdk-registered.sh` is gated on ITSELF: it reads
      `~/.cmake/packages/Zephyr-sdk/` for an entry whose CONTENT is this SDK's
      `cmake` dir, and registers only when there is none. Keying on the content
      rather than "the directory is non-empty" is what makes it per-artifact —
      a host registered for a DIFFERENT version has a non-empty directory and
      still cannot build this line.
      `-c` only, never `-h`: `-c` writes the registry entry, `-h` installs host
      tools, is the expensive half and is reported to FAIL in a container. A
      step that is cheap and always safe must not inherit the gating of one
      that is neither — which is the whole bug, stated once.
- [x] **The same defect existed TWICE, one layer apart**, and only the outer one
      is what the issue quotes. `just/zephyr-setup.just` skips all of
      `setup.sh` when the WORKSPACE exists; `setup.sh` then skips `install_sdk`
      — the thing that registers — when the SDK DIRECTORY exists. Fixing only
      the verb would have left a host with an unpacked, unregistered SDK
      unrepairable by `setup.sh` itself. Both call the helper now.
- [x] Removing the registry entry and re-running the verb restores it, without
      `--force` and without a re-download. Measured end to end: registry
      emptied, `just zephyr setup` with no flags, and the log reads
      `Zephyr workspace already present` (the skip still fires, correctly),
      then `registering`, then the entry is back — while every source step
      reports `already present (skip)`, so nothing was re-fetched.
      The version comes from `zephyr/SDK_VERSION`, Zephyr's own statement of
      what the line needs and what the doctor block beside it already reads,
      rather than a fourth literal `0.16.8`.
      NOT `|| true` on that call: 1279's complaint is that this failure is
      invisible until a build dies inside `FindZephyr-sdk.cmake`, and swallowing
      it would rebuild that defect one line lower.

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
