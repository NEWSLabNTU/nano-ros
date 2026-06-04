> **Post-Phase-218 (audit-report callout)**: References below to
> `scripts/install-nros.sh` reflect the pre-218 install. Canonical
> install is now `just setup-cli` + `source ./activate.sh`. Preserved
> as historical record.

BLOCKERS
None.

FRICTION
- `just doctor tier=all` (doc's "Diagnose missing tools" step) exits 1, reporting 9 module failures on a warm host. Most damaging mismatch: the zenohd/qemu checks look at `build/zenohd/` and `build/qemu/`, not the `~/.nros/sdk` store the doc told the user to install into. A user who just ran `nros setup native` and then runs `just doctor` per the doc sees `[MISSING] zenohd` despite zenohd being installed and working via the doc's shim. The doctor and the doc are pointing at different install locations.

CLARITY
- Step 1 install URL (HTTP 200), pinned 0.3.7 in `scripts/install-nros.sh` matches host: CLEAR.
- Step 2 + table + flags: CLEAR. Every dry-run / list / licenses / tool / source / rmw call resolves; `--rmw` default zenoh confirmed.
- Heads-up zenohd PATH shim: CLEAR. Resolves and runs `zenohd v1.7.2`.
- Contributor section: CLEAR. `just freertos|nuttx|threadx_linux setup` all reach "ready; locked in nros-sdk.lock" exit 0.

MISSING STEPS
- Heads-up paragraph names zenoh + xrce daemons but not cyclonedds — a Pattern A reader on `--rmw cyclonedds` might search for an absent daemon (cyclonedds is in-process).
- `~/.nros/sdk` store path never named in prose; only `~/.nros/bin` is.

WORKS
- nros 0.3.7 CLI, --list / --licenses / --dry-run / --tool / --source / --rmw.
- All 7 doc-table boards dry-run cleanly.
- `nros setup native [--rmw {zenoh,xrce,cyclonedds}]` all reach "ready".
- `just {freertos,nuttx,threadx_linux} setup` all reach "ready".
- Every in-doc link resolves (8 in-tree + 2 GitHub tree URLs, all 200).
- `just docker {build,shell,test-qemu}` recipes exist.

Acceptance bar (0 BLOCKERS) MET.

LAST COMMAND: nros setup 2>&1 | head -25
LAST EXIT CODE: 0
