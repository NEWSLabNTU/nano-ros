BLOCKERS
None. Bar met. The page can be followed end-to-end with the artefacts and CLIs it names.

FRICTION (both fixed in `2bb0dfdcc`)
1. **Port mismatch.** Tutorial quoted `examples/qemu-arm-nuttx/c/talker/nros.toml` with `locator = "tcp/10.0.2.2:7552"` but the very next paragraph runs `just nuttx zenohd` which binds **7452** (verified at `just/nuttx.just:532` and the CLAUDE.md per-platform port table). Internal contradiction inside the page. Fixed: switched citation to `examples/qemu-arm-nuttx/rust/talker/nros.toml` (port 7452 — what `just nuttx talker` actually consumes) + added a short note on the per-language port pattern (Rust 7452, C 7552, C++ 7652) and how to retarget when booting C/C++ directly.
2. **`$NUTTX_APPS` vs `$NUTTX_APPS_DIR`.** Page referenced `$NUTTX_APPS` in Project-layout + manual symlink path, but `.envrc` exports `NUTTX_APPS_DIR`. Copy-paste readers got an empty expansion. Fixed: standardized on `$NUTTX_APPS_DIR` everywhere (9 occurrences).

CLARITY
- CLEAR on prereqs, layout, configure, build, run, readiness signal, and the auto-configure glue. The Rust-vs-`make`-driven split is well-marked.

MISSING STEPS (NITs, deferred)
- No instructions on where to flip the locator if 7552/7652 doesn't match (now covered by the new port note from this commit, but the explicit "edit `nros.toml` to retarget" snippet stays implicit).
- No explicit pointer to the `$NUTTX_DIR/nuttx` ELF that the QEMU command line consumes.

WORKS
- `nros setup qemu-arm-nuttx --rmw zenoh`.
- `just nuttx setup`.
- `just nuttx zenohd` (post-`build/zenohd/zenohd` fix; binds 7452).
- All referenced repo paths (board defconfig, `integrations/nuttx/`, `scripts/nuttx/stage-external-apps.sh`).
- `EXPECTED_PROGNAMES` matching the in-text PROGNAME list.

The only command failure (`just nuttx talker`, exit 1) was the known nested-worktree-under-same-repo cargo-walk artefact (cargo walks past the worktree's `Cargo.toml` excludes and finds the main repo's root). Not a doc bug; tracked as F1 in `phase-208-followups.md`.

Acceptance bar (0 BLOCKERS) MET.

LAST COMMAND: just nuttx talker
LAST EXIT CODE: 1 (worktree-only artefact, not a doc bug)
