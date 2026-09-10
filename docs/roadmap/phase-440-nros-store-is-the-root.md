# Phase 440 — the store is the root

**Status (2026-09-09). W1 LANDED; W2 CLOSED by deletion (issue 1248); W3-W8 open.** Implements
[RFC-0095](../design/0095-nros-store-is-the-root.md). The campaign home for
moving nano-ros from "a repository the user places" to "an artifact the user's
CLI provisions", and for the provisioning-root cleanup that has to happen first
either way.

**Prior phases:** 435 (provisioning resolves by axis), 422 (provisioning has one
path), 413 (CI workflow user parity), 431 W1 (the CLI ownership guard), 383 W2.d
(preflight stage 3).

## Why this exists

Two audiences, one of which the tree cannot currently serve. A **contributor**
clones nano-ros, edits it, and builds from inside it. A **user** should install
a CLI and work in their own project, against a nano-ros version their project
pins — and today every provisioned path resolves relative to a checkout, with
`$repo_root/../nano-ros-workspace` as the fallback: a path relative to a clone
the user does not have.

The cleanup is not speculative work for that future. Three of its steps are
already paying:

* the **tier-2 lane** has produced no runtime verdict in nine consecutive runs,
  two of them killed by a Zephyr workspace provisioned inside a *second*
  nano-ros checkout (RFC-0095 D1);
* `check-box-sync-covers-tracked-source` reports **2914 tracked paths** that
  would not reach the box mirror, because provisioned source and build output
  share directories and no name-based rule can separate them;
* a gate that assumes a distrobox ran on the push lane for everyone.

## Work items

### W1 — one resolver, store arm added, behaviour unchanged — **LANDED**

The Zephyr workspace chain had three copies (`just/zephyr.just:19`,
`scripts/build/west-fixtures.sh`, `scripts/check-tier-preconditions.sh`), which
is the two-spellings shape this repo keeps paying for. They now read
`scripts/lib/zephyr-workspace.sh`, which also carries the
`$NROS_STORE/workspaces/zephyr/<version>` arm **below** the existing ones so
nothing moves yet. `just` cannot call a shell function, so it CALLS the helper
as a command (`shell("scripts/lib/zephyr-workspace.sh … resolve-or-default")`)
rather than restating the ladder.

**What the fold had to decide, because the three DISAGREED.** Measured over a
9-shape × 2-line matrix of synthetic hosts, before and after: 39 of 54 cells are
byte-identical, and **all 15 that moved are cells where the three resolvers
already gave different answers** — nothing the tree agreed on changed. The
disagreements were real: on the 4.4 line `just zephyr` named the 4.4 sibling
while west-fixtures resolved the 3.7 in-tree workspace and
check-tier-preconditions reported no workspace at all; west-fixtures also let a
set-but-not-yet-created `NROS_ZEPHYR_WORKSPACE` fall THROUGH to a different
tree. The reconciliation, stated in the helper's header: the override wins
unconditionally (it is the install target too), a candidate is a workspace when
it holds `zephyr/`, and the ladder is version-aware one line at a time.

*Acceptance:* one function; the three call sites read it; `just check fast`
green (276 ran, 9 skipped for absent provisioning, 0 failed);
`check-zephyr-workspace-resolvers` refuses a fourth spelling, ratcheted through
`.config/zephyr-workspace-resolvers.txt` and mutation-tested in both directions.

*What W1 did NOT do:* 29 files still spell the ladder themselves — the CLI's own
Rust resolver, the nros-tests harness, four more shell ladders, three Python
ones and the `scripts/zephyr/*-patch.sh` legacy-tree rung. They are recorded
with reasons in the ratchet rather than folded, because most want the store
inversion (W4) or the D1 generalisation (W5) first. The ratchet only shrinks.

### W2 — the box-sync gate's lane — **CLOSED by deletion (issue 1248)**

The lane question had three candidates and the answer was that the gate should
not exist. `ros2-box-sync.sh` is retired, so there is no mirror to be unfaithful
to; the gate is deleted rather than relaned, and its name comes out of
`.config/gate-registry-baseline.txt` as the deliberate retirement that file
documents.

The principle it violated is now RFC-0095 D12: the build system encodes no
environment shape, and asks only whether ROS 2 ament packages are discoverable.
Both attempts recorded here were treating a symptom — moving the gate to
`build-serial` (refused by `check-gate-visibility`, since a build-tier gate must
FAIL in a pristine worktree and this one passed) and narrowing its sweep (which
would have silenced #758's real rescue).

*Acceptance (met):* the gate, the mirror and the `.nros-box-tree` marker are
gone; `ros2-box-env.sh` is 48 lines from 260 and sets only the store split;
`check-gate-lists` 283 fast / 21 build-serial and `check-gate-visibility` 21
acknowledged both green. Issue #0925 closed as moot.

### W3 — `third-party/` holds tracked submodules and nothing else — **LANDED, one exception declared**

Measured rather than assumed, and most of it was already done: `make/` and
`ninja/` provision into the store and their consumers already read it there
("the store rather than `third-party/make/`", `jobserver-pool.sh`). What
remained was 1.2 MB of residue and two `.gitignore` lines that made the
directory look like it still held provisioning.

`external/` turned out to be provisioned by NOTHING and consumed by NOTHING —
every apparent consumer is NuttX's own `apps/external/` or prose. It is 287 MB
of debris on this host; its `.gitignore` entry stays for now because deleting
another operator's untracked data is not this phase's call to make.

`check-third-party-is-submodules` is the ratchet. Submodule parents come from
`.gitmodules`, never a directory walk, so an uninitialised submodule still
counts (a bare clone has empty dirs), and an unreadable `.gitmodules` REFUSES
rather than reporting OK over a reading that found nothing.

**One declared exception: `ros`.** `[source.rosidl]` has
`dest = "third-party/ros/rosidl"` and `msg_to_cyclone_idl.py` resolves it as its
last ladder rung so the cyclone msg→IDL step works with no ROS install. Moving
it needs `dest` to be able to name the STORE — RFC-0095 D2/D4, which is W4. So
it is declared with a reason and somewhere to go, rather than the rule being
weakened to fit it. The list may only shrink.

*Acceptance (met):* gate green (9 submodule parents, 1 exception). Mutations —
a new provisioning root under `third-party/` → RED naming it and its size; an
unreadable `.gitmodules` → rc 2, refusing rather than passing. `.gitignore`
loses two entries and gains none.

### W4 — provisioned workspaces move to the store — **RESOLUTION INVERTED; the data move is operational**

W1 built the store arm and left it last. W4 promotes it, which is RFC-0095 D4:

```
$NROS_ZEPHYR_WORKSPACE  ->  $NROS_STORE/workspaces/zephyr/<version>  ->  <checkout-relative>
```

and moves the INSTALL target (`nros_zephyr_ws_default`) into the store, so the
next `just zephyr setup` provisions there rather than beside a clone.

**Nothing breaks on a host provisioned before this.** An absent candidate is
skipped, so a populated `zephyr-workspace/` still resolves exactly as it did —
measured on this host: `resolved` unchanged for both the 3.7 and 4.4 lines,
`default` moved to the store. The store starts mattering the first time `setup`
runs, which is when it becomes the answer.

The checkout arm STAYS, last, on purpose (D4): a host provisioned earlier keeps
working with no migration step, and a contributor patching a Zephyr module still
points `NROS_ZEPHYR_WORKSPACE` at their own tree.

*Acceptance (met, for the resolution half):* ladder order asserted directly —
store above the checkout arms on both version lines, and an override ends the
ladder. With the store populated it wins over a populated checkout tree; with it
empty the checkout tree still resolves.

*Not done here, deliberately — the DATA move is operational, not a code change.*
Relocating 4.1 GB of provisioned source is `mv` plus a re-resolve on whichever
host runs it, and it is safe to do at any time BECAUSE the ladder now prefers
the store: a host that moves its tree is answered by the store arm, and one that
does not is answered by the checkout arm. Neither needs this repo to change
again. The 140 GB of build output beside it belongs to whoever built it and does
not move.

*Still open, and it is the last mechanism this phase needs:* `[source.rosidl]`
has `dest = "third-party/ros/rosidl"`, and `dest` cannot name the store, so the
`ros` exception W3 declared cannot be retired yet. That is one mechanism —
store-aware `dest` in `nros-sdk-index.toml` — and it serves every future
`[source.*]`, not just rosidl.

### W5 — D1 gated for every provisioned tree — **LANDED (generalised ahead of W4)**

`scripts/check-zephyr-workspace-checkout.sh` asked about ONE root, so it
reported a property of that root rather than of the host: `esp-idf-workspace`
could sit inside a foreign checkout and nobody would hear about it until a
fixture build fifteen minutes in. It now walks a declared list —
`zephyr` / `esp-idf` / `external` — and reports EVERY offending root rather than
stopping at the first.

Done before W4 on purpose: the list is the honest shape while the tree really
carries four roots, and RFC-0095 D2 collapses it to one store-resolved root
later, at which point the per-tree spellings go with it.

The three silent cases still MIRROR the ownership guard exactly, so this can
never be stricter than the thing it front-runs — the marker and the lexical walk
are kept identical to `stale_guard.rs` deliberately.

*Acceptance (met):* six branches exercised — zephyr root in a foreign checkout →
refuse; **esp-idf root in a foreign checkout → refuse (new coverage)**; outside
any checkout → silent; foreign checkout with no `packages/cli` → silent;
`NROS_SKIP_STALE_CHECK=1` → silent; own tree → silent. With two roots foreign at
once it names both.

*Not done:* the file keeps its `zephyr` name and its registration line, because
renaming touches `check-tier-preconditions.sh`, which phase-440 W1 (PR #807) is
editing. Rename when that lands — a conflict there would be self-inflicted.

### W6 — the store can be inspected and shrunk — **LANDED**

`nros store list` / `nros store gc --older-than <d>` / `nros toolchain uninstall
<ver>`. Additive-by-design storage needs a verb to reclaim, and
reference-counting against pins scattered across a filesystem is not reliable,
so this is explicit and inspectable rather than clever.

*Acceptance:* `--dry-run` is the DEFAULT and the tests assert it; `uninstall`
refuses while a known pin names the version; `list` reports size and last-used
so a human can decide. A test proves gc never removes an entry a pin names.

**What landed** (`orchestration/store.rs` + `cmd/{store,toolchain}.rs`, 12 tests
in `tests/store_reclaim.rs`):

* the store ROOT has one resolver — `store::root()`, `$NROS_STORE` →
  `$NROS_HOME` → `~/.nros`, with `sdk_store::{store_root,front_dir}` now derived
  from it instead of repeating the arms (D2's "never an absolute literal");
* `gc` removes only entries carrying `.nros-provenance`. The others are LISTED
  with their size and left alone: the legacy flat prefix `nros sdk-path` still
  resolves through (issue 0628) and `<version>.src` source trees, which are 1.5 G
  of the 4.4 G store on the host this was written on — nothing needs them, but a
  source tree is also where somebody's local patches would be;
* **`--delete` refuses when NO pin file was consulted**, not just when one names
  the entry. The store is shared while pins are per-project, so an empty pin set
  from the wrong cwd means "I looked in the wrong place" at least as often as it
  means "nothing needs these". `--ignore-pins` is the explicit way past it;
* **`toolchain uninstall` therefore refuses on today's tree**, which is the
  honest answer rather than a limitation: `toolchains/<ver>/` and
  `nros-toolchain.toml` are both W7's, so nothing yet exists that could establish
  that no project pins a version. The pin reader already accepts
  `nros-toolchain.toml` and takes ANY string value in it as a version, so W7 may
  choose its schema without disarming the guard;
* "last used" is the newest `atime` over an entry's regular files, with the
  timestamp's PROVENANCE printed beside it (`read` vs `install`) because under
  `noatime` there is no usage signal at all and the number would otherwise read
  as one. Two traps found by measuring rather than reasoning: the scan's own
  read of `.nros-provenance` made every installed entry report "used 1s ago"
  until those markers were excluded, and at one-second timestamp resolution the
  test for that passed with the fix removed.

### W7 — the per-project pin, and dispatch by it — **SMALLER THAN WRITTEN**

Measured on main rather than assumed, and two thirds of this item already exist.

`scripts/install.sh` (phase-431 W4) downloads a **versioned release asset**
(`.tar.zst`, `NROS_INSTALL_VERSION`, `NROS_INSTALL_URL` for a mirror or an
air-gapped host) into `$NROS_HOME/sdk/nros/<version>` and fronts it at
`$BIN/nros` through the binary's own `sdk-front` — not a symlink written by the
installer, so there is one implementation.

So the store is ALREADY version-keyed for the CLI, and RFC-0095 D8's open
question — *release artifact or git checkout?* — is **answered in practice: a
release artifact.** That settles D6's second row: auto-fetching a toolchain is
defensible here, because it is a download and not a ten-minute `cargo build`.
`toolchains/<version>/` in D2 is a RENAME of `sdk/nros/<version>`, not new
storage.

What is genuinely missing is the half that makes it per-project:

* `nros-toolchain.toml` beside the user's project, and pin-on-first-build
  (RFC-0095 D9 — the `Cargo.lock` rule of issues 0359/0378, one layer up);
* dispatch: the fronted `nros` reads that pin, ensures that version, `exec`s it.
  Three jobs and no more (D8) — it must keep working when it is OLDER than the
  toolchain it launches.

W6's `toolchain uninstall` already reads `nros-toolchain.toml` and accepts any
string value, deliberately, so this may pick its schema without disarming that
guard.

*Acceptance:* two projects pinning two versions build side by side on one host;
`nros self update` leaves both outputs byte-identical; a pin bump re-stales
generated code through the existing input signature (#182) rather than a second
mechanism; rollback re-downloads nothing. The shim keeps working when older than
its toolchain — tested, not assumed.

**LANDED.** `orchestration::pin` is the file, `orchestration::dispatch` is the
launcher, and it runs from `main` **before clap** — that ordering is what "older
than what it launches" means in code, not a style choice, and
`an_older_launcher_execs_the_newer_pinned_toolchain` proves it by handing the
launcher a flag its OWN clap would reject and asserting the stub received it.
Four things worth recording, each of which a reading-only implementation would
have got wrong:

* **The pin names the STORE version (`0.5.0-nros1`), never the crate version
  (`0.5.0`).** Its whole job is to name a directory, so
  `pin::running_version` reads it off the running binary's own store prefix —
  both `toolchains/<v>` and today's `sdk/nros/<v>`, because D2's rename has not
  happened and a launcher taught only the new name recognises no installed host.
  A binary in NEITHER shape (a contributor's `packages/cli/target/**` build)
  has no store version and pins NOTHING, loudly. Inventing one would write a
  pin whose next build dispatches to a directory nobody can create.
* **`write` refuses to overwrite, and that is the D9 half easy to leave out.**
  "Write the version it used" on every build moves the pin on every
  `nros self update` — the silent rebuild D7 exists to forbid.
* **D8 job 2 delegates to `scripts/install.sh`, which the release asset now
  carries at `share/nros/install.sh`** (`release-nros.yml`, one `cp`). A Rust
  downloader would be a second answer to "which asset, which checksum, which
  prefix", in the one place that cannot see the store accumulate — the same
  argument `cmd::sdk_front` already makes for the front. An asset predating that
  staging carries no installer and REFUSES, naming the pin, the paths it looked
  in and the `install.sh --version <v>` line; that is the arm the real-binary
  test exercises, because a copied binary brings no `share/`.
* **A dispatched child is marked (`NROS_TOOLCHAIN_DISPATCH=<version>`).** A
  store entry whose directory name disagrees with the binary inside it is
  otherwise an exec loop — the one failure mode a launcher must not have.

The stale/ownership guard is untouched (RFC-0095 D5, and this phase's own
non-goal). Dispatch reaches the same conclusion independently and at a different
seam: it declines whenever `abi_guard::find_monorepo_root` answers, so a
contributor never meets it.

*Still open, and named rather than implied:* the acceptance line's "a pin bump
re-stales generated code through #182" is ARGUED, not measured — #182 keys the
contributor path on the source stamp, and the user path would key on the
toolchain version, which is the same machinery pointed at a different input but
has no test on this side of the line. Two projects building side by side needs a
real release asset, so it belongs with W8's remaining user-path probe.

### W8 — the one-line installer — **ALREADY LANDED (phase-431 W4)**

`curl -fsSL …/scripts/install.sh | sh` exists and is gated by
`check-nros-installer` (`tests/nros-installer-tests.sh`), which serves an asset
over a local HTTP server and installs it into a scratch store.

This phase claimed it as open because the phase doc was written from the RFC
rather than from the tree. Recorded so nobody builds it twice.

*Remaining, and small:* `just probe bootstrap` covers the CONTRIBUTOR path. The
user path — install, pin, build a `nros new` project **without cloning
nano-ros** — is what W7's pin makes testable, so it lands with W7 rather than
here.

### Store-aware `dest` — **LANDED. The last mechanism, and W3's exception is retired**

`[source.*]` had one shape, a workspace-relative `dest`, so a provisioned source
could only land inside a checkout. That is what forced W3 to DECLARE `ros` an
exception in a ratchet whose whole point is that `third-party/` holds tracked
submodules and nothing else.

`SourceLocation` is `workspace` (default) or `store`. A store source's path is
DERIVED — `$NROS_STORE/sources/<name>/<version>`, off W6's `store::root()` so
there is one spelling — mirroring `tool_dir` rather than inventing path syntax.
The validator refuses both directions: a workspace source with no `dest`, and a
store source WITH one.

`nros sdk-path --source <name>` is the consumer bridge, for the reason the tool
arm exists: a consumer asks, never spells. rosidl flipped, and
`msg_to_cyclone_idl.py` asks — with the pre-phase-440 `third-party/ros/rosidl`
kept BELOW the store rung, the same shape W4 used for workspaces, so a host
provisioned earlier needs no migration step.

**`EXCEPTIONS` in W3's ratchet is now EMPTY** (`0 declared exception(s)`), and
the gate still refuses a new provisioning root — verified with the list empty,
because an exemption list that has stopped being exercised is where a rule
quietly stops applying.

## Order, and what is worth doing regardless

W1 → W3 → W4 → W5 is the cleanup, and it stands on its own: it fixes the tier-2
blocker, empties the box-sync backlog, and removes a class of "which of the four
roots did this rule forget". W6 → W8 is the user-facing half and depends on the
cleanup, not the reverse.

W2 is closed: the gate it asked about is deleted with the mirror it guarded
(issue 1248, RFC-0095 D12).

## Non-goals

* **Changing the ownership guard.** RFC-0095 D5 — it already distinguishes the
  two audiences correctly, and the correct move is to leave it alone.
* **Auto-provisioning platform prerequisites.** RFC-0095 D6 keeps RFC-0065 D2's
  refuse-and-name-the-remedy. Only the toolchain is fetched automatically, and
  only if it ships as a release artifact.
* **Removing the checkout-relative resolution arm.** It stays last, so a
  contributor patching a Zephyr module still points at their own tree.
