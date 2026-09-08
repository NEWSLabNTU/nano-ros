# Phase 440 — the store is the root

**Status (2026-09-09). W2 landed; W1 and W3–W8 are open.** Implements
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

### W1 — one resolver, store arm added, behaviour unchanged

The Zephyr workspace chain has three copies today (`just/zephyr.just:19`,
`scripts/build/west-fixtures.sh`, `scripts/check-tier-preconditions.sh`), which
is the two-spellings shape this repo keeps paying for. Fold them into one
helper, add the `$NROS_STORE/workspaces/<name>/<version>` arm **below** the
existing ones so nothing moves yet.

*Acceptance:* one function; the three call sites read it; `just ci gate` green;
a gate refuses a fourth spelling (the `check-cli-source-dirs` shape). With no
store populated, every existing host resolves exactly what it resolved before —
proven by printing the resolved path on a provisioned host and diffing against
the pre-change value.

### W2 — the box-sync gate leaves the push lane — **LANDED 2026-09-09**

Moved to `build-serial`. Not exempted and not retired: it stays in a registry so
it cannot be deleted unnoticed (issue 1071), and it still runs on the tier
schedule and dispatch reach. Exempting it would have required shrinking the
name-set baseline, which asserts a retirement that has not happened — the
ratchet was right to refuse.

*Acceptance (met):* `check-gate-lists` OK — 282 fast, 22 in `build-serial`;
the gate's own comment records why it is not a push-lane property.

### W3 — `third-party/` holds tracked submodules and nothing else

Move `make/`, `ninja/`, `ros/` to the store; retire `external/` into
`$NROS_STORE/fetch/`; **delete** the 23 M `external/PX4-Autopilot`, a stray
duplicate of the 68 G tracked submodule at `third-party/px4/PX4-Autopilot`.

*Acceptance:* every path under `third-party/` is a tracked submodule or tracked
content — asserted by a gate, so a fourth provisioning root cannot land there
quietly. `.gitignore` loses the corresponding entries rather than gaining any.

### W4 — provisioned workspaces move to the store, version-keyed

`zephyr-workspace/{zephyr,modules}` (4.1 G of source; the sibling 140 G is build
output and stays with whoever built it) becomes
`$NROS_STORE/workspaces/zephyr/<version>/`. Same for esp-idf.

*Acceptance:* a second checkout on the same host provisions **nothing** and
builds; `du` of the store shows one copy; the box-sync gate needs zero
`--include` rules for provisioned trees, because none sit under the sync root.

### W5 — D1 gated for every provisioned tree

`scripts/check-zephyr-workspace-checkout.sh` (landed with W2's commit) already
refuses a Zephyr workspace inside a foreign checkout at precondition time.
Generalise it once the trees share a root, and delete the per-tree spelling.

*Acceptance:* the check names any provisioned root, not one; its five branches
stay exercised (nested → refuse; outside any checkout → silent; second checkout
without `packages/cli` → silent; `NROS_SKIP_STALE_CHECK=1` → silent; own tree →
silent), so it can never be stricter than the ownership guard it front-runs.

### W6 — the store can be inspected and shrunk

`nros store list` / `nros store gc --older-than <d>` / `nros toolchain uninstall
<ver>`. Additive-by-design storage needs a verb to reclaim, and
reference-counting against pins scattered across a filesystem is not reliable,
so this is explicit and inspectable rather than clever.

*Acceptance:* `--dry-run` is the DEFAULT and the tests assert it; `uninstall`
refuses while a known pin names the version; `list` reports size and last-used
so a human can decide. A test proves gc never removes an entry a pin names.

### W7 — `toolchains/<version>/`, the pin file, and the shim

The rustup-shaped core: `nros-toolchain.toml` in the user's project;
`$NROS_STORE/toolchains/<ver>/`; a shim whose only jobs are read the pin, ensure
the toolchain, `exec` it. First build **writes** the pin it used (RFC-0095 D9,
the `Cargo.lock` rule one layer up). `nros self update` moves the shim and
nothing else.

*Acceptance:* two projects pinning two versions build side by side on one host;
`nros self update` leaves both outputs byte-identical; a pin bump re-stales
generated code through the existing input signature (#182) rather than a second
mechanism; rollback to the previous pin re-downloads nothing. The shim keeps
working when it is OLDER than the toolchain it launches — tested, not assumed.

### W8 — the one-line installer

`curl … | sh` places the shim and nothing else. Closer than it looks:
`scripts/bootstrap.sh` already unpacks a CLI with no checkout to build it, and
`nros sdk-front` already fronts a version at `$NROS_HOME/bin/<name>`.

*Acceptance:* `just probe bootstrap`'s pristine-container flow covers the user
path, not only the contributor path — a fresh container installs, pins, and
builds a `nros new` project without cloning nano-ros.

## Order, and what is worth doing regardless

W1 → W3 → W4 → W5 is the cleanup, and it stands on its own: it fixes the tier-2
blocker, empties the box-sync backlog, and removes a class of "which of the four
roots did this rule forget". W6 → W8 is the user-facing half and depends on the
cleanup, not the reverse.

W2 is landed and was independent of both.

## Non-goals

* **Changing the ownership guard.** RFC-0095 D5 — it already distinguishes the
  two audiences correctly, and the correct move is to leave it alone.
* **Auto-provisioning platform prerequisites.** RFC-0095 D6 keeps RFC-0065 D2's
  refuse-and-name-the-remedy. Only the toolchain is fetched automatically, and
  only if it ships as a release artifact.
* **Removing the checkout-relative resolution arm.** It stays last, so a
  contributor patching a Zephyr module still points at their own tree.
