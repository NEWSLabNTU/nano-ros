BLOCKERS
- config.toml does not exist — Configure step instructs editing `config.toml`; the in-tree file is `nros.toml` with a different schema. Anything a user copies from the doc into `config.toml` is silently ignored (board crate reads `nros.toml` via `Config::from_toml`).
- Doc TOML schema is wrong — doc shows `[network] ip/mac/gateway/prefix` + `[zenoh] locator/domain_id`. Real schema is `[node] domain_id` + `[[transport]] kind="ethernet" ip="10.0.2.10/24" mac gateway rmw locator` — CIDR is in `ip`, no separate `prefix`. User edits per doc produce an un-parseable file.
- `cargo build --release` from the example dir fails in this worktree: "current package believes it's in a workspace when it's not; workspace: /home/aeon/repos/nano-ros/Cargo.toml". The wt-root Cargo.toml has the package under `exclude`; cargo walks up to the outer repo manifest, which member-lists the package under a path that doesn't include `.claude/worktrees/…`. Per audit rules, the build/run thread is stopped. Probably wouldn't bite a plain clone → FRICTION there, but it blocks completing the audit here.

FRICTION
- Stale host `nros` (0.3.1) couldn't parse current `nros-sdk-index.toml` (`shallow = true`, needs CLI ≥ 0.3.2). The pinned installer script (0.3.7) fixes it. Hosts with an older CLI hit a cryptic TOML schema error from `nros setup`.
- Doc's `curl … | sh` install is blocked by the safety classifier in this env; `sh scripts/install-nros.sh` is equivalent (recorded, not a doc bug).

CLARITY
- "When to use this path" / Prereqs / Readiness / Constraints / Next — CLEAR.
- Project layout — VAGUE: lists `config.toml` + `generated/` neither of which exists in-tree (real: `nros.toml`; `generated/` post-build only).
- Configure — MISSING: wrong filename + wrong schema.
- Build — CLEAR (instruction-wise) but unverifiable in worktree.
- Run — VAGUE: expected banner "nros Bare-Metal Cortex-M3 Talker" is not in `src/main.rs` (only `nros_info!` lines for "Declaring publisher on /chatter…", "Publisher declared", "Published: N"). Banner is fictional.

MISSING STEPS
- No mention that the board crate parses `nros.toml`, not `config.toml`.
- No mention that `nros setup` requires CLI ≥ 0.3.2.
- No mention of `nros generate-rust` (or that build.rs runs codegen); `generated/` is shown in layout but the trigger is implicit.

WORKS
- `sh scripts/install-nros.sh` → nros 0.3.7 in `~/.nros/bin/`.
- `nros setup qemu-arm-baremetal --rmw zenoh` → provisions arm-none-eabi-gcc 13.2-nros1, qemu 11.0.0-nros2, zenohd 1.7.2-nros1, zenoh-pico + mbedtls submodules.
- `.cargo/config.toml` runner matches the doc.
- Board crate path `packages/boards/nros-board-mps2-an385/` exists.

LAST COMMAND: cargo build --release --manifest-path examples/qemu-arm-baremetal/rust/talker/Cargo.toml
LAST EXIT CODE: 101
