---
id: 2764
title: "A Zephyr workspace silently loses nano-ros's patch set: the manifest sync resets it, and nothing says a workspace lacks it"
status: open
type: bug
severity: medium
area: [zephyr, build, tooling]
related: [issue-2249, issue-2476, issue-2697]
found: 2026-11-20
---

## Measured

Host 241, 2026-11-20 (reported by the 241 manager):

1. **3.7 store workspace, after `scripts/zephyr/sync-workspace-manifest.sh`
   run on its own.** The sync set nano-ros's in-place edits aside before
   `west update` (issue 2476) and relied on its CALLER to re-apply
   `scripts/zephyr/patches/<line>.sh`. Run directly, it left an unpatched
   workspace, which failed only later, at configure: the LLEXT EDK
   generator-expression error that `llext-edk-conditional-patch.sh` exists to
   remove. `check-zephyr-pin-drift.py` printed exactly that standalone command
   as its remedy for drift, so the trap was the documented path.
2. **4.4 store workspace with no patch set at all.** No
   `nano-ros: staticlib only` marker (`cargo-features-patch.sh`), so every Rust
   image failed at configure. How it got there was not measured. The
   candidates are the same standalone sync on the 4.4 line, or a provisioning
   path that skipped the set. The issue stays open until that is known. The
   probe below names the state whatever the cause.

What the two have in common is that nothing recorded whether a workspace
carried its line's patch set. A workspace without it looked like any other
until a build failed, many minutes in, with an error about Zephyr internals.

## Fix

- **The sync applies the line's patch set itself.** After a successful
  `west update` it reads the line from `<ws>/zephyr/VERSION` and runs
  `<checkout>/scripts/zephyr/patches/<line>.sh <ws>`. It fails, saying the
  workspace is UNPATCHED, when the set fails, when the line is unreadable, or
  when the checkout has no set for it. The setup callers still run the set
  after the sync. That second run is idempotent.
- **Each patch set records that it ran** as its last step, in
  `<ws>/.west/nros-patchset`: the line plus a digest of the set's sources (the
  set file and every `scripts/zephyr/*.sh` it names). `scripts/lib/zephyr-patchset.sh`
  is the one reader and writer. A store workspace is shared, so the record
  keeps one digest row per distinct set applied. Patches are idempotent and
  additive, so a checkout with an older or newer set is satisfied once it has
  run its own. The sync drops the record when an update moves the projects.
- **`check-zephyr-pin-drift.py` probes it.** It already runs in the gate-lane
  preflight, so a gap is named before a build reaches configure:
  - this checkout's set is recorded: ok;
  - another set is recorded (a patch added since), or another line: FAIL;
  - no record, and no pinned project has a local edit: FAIL ("carries NONE");
  - no record, but projects are edited: NOTE. That is every workspace patched
    before records existed, and one run of the set records it.

  Each FAIL names `bash scripts/zephyr/patches/<line>.sh <ws>` as its remedy.

## Acceptance

- `tests/zephyr-sync-workspace-manifest-tests.sh` (row 11): a sync that
  updated applies the set once, a failed update applies none, and a failing
  set, an unreadable line or a missing set each fail the sync. Run against
  main's sync script, those rows fail 7 of 7.
- `check-zephyr-pin-drift.py`'s self-test covers the record cases with the
  real recorder and reader. A mutation that reads every record as ok fails it.
- On host 139, both store workspaces read NOTE before re-patching. After
  `patches/3.7.sh` ran, the 3.7 workspace reads ok.
