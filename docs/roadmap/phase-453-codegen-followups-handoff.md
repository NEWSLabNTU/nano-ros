# Phase 453 — codegen campaign follow-ups: landing and handoff

**Status (2026-09-18). W1 done; W3 triaged; W2 in flight.** Phase 432 (RFC-0091, "one codegen producer,
many language packs") is finished. Its archive lands with PR #937. This doc
exists so the next session, human or agent, can pick up from `main` without
reading a chat log. Every item below is on origin. Nothing lives only on one
machine.

**Relates to:** [RFC-0091](../design/0091-one-entry-codegen-producer-many-language-packs.md),
[RFC-0068](../design/0068-language-neutral-codegen-ir.md).

## W1 — land the four open PRs

**DONE 2026-09-18.** All four merged, together with the handoff PR #945.
Each was rebased onto `main` first, and each carried the gate caveat below;
nothing was ejected.

Each is rebased onto `main` as of 2026-09-11 and authored with a local test run.
`just ci gate` never finished cleanly in any of their worktrees, but in every
case the cause was the environment: unprovisioned CycloneDDS or XRCE sources,
or the host's low-memory killer. So the merge queue is the first COMPLETE gate
for each. Read an ejection before re-queueing.

| PR | branch | what it does |
| --- | --- | --- |
| #934 | `fix/entry-sched-bind-rc` | Both entry packs fail closed when `nros_cpp_bind_*_sched` refuses a binding. The C entry banner names the runner the entry actually calls. |
| #936 | `test/rv-virt-threadx-c-workspace` | A pure-C ThreadX workspace entry on rv-virt-threadx: fixture row, matrix cell, and an `entry_e2e` boot variant. The runtime cell passed solo on QEMU riscv64. |
| #937 | `docs/phase-432-closeout` | The silent `_ => emit_cpp` dispatch fallback becomes an explicit refusal. RFC-0091 §8/§9 and the book now admit that adding a language takes Rust. RFC-0068 gains Amendment 1 (`TargetProfile` retired). Phase 432 is archived. |
| #938 | `fix/1285-followup-rtos-substring` | No RTOS is ever read from a key's spelling. It CHANGES some answers deliberately, for example `native_sim/native/64` → zephyr and unknown ids become errors; the full table is in the PR body. |

**Done when:** all four are merged, or closed with a reason recorded here.

## W2 — the open issues this campaign produced

**IN FLIGHT 2026-09-18.** Every issue below is `status: open` on `main`, and each
is being worked in its own branch now. Two corrections to the original list:

- **1306 is a DUPLICATE of [1336](../issues/1336-cli-source-stamp-unwatched-in-worktree.md)**,
  filed a day later by phase-454 W3 with better evidence and cross-references
  (0419, 0466, 0561, 0627, 0921). 1336 is the canonical id; 1306 is being closed
  as a duplicate, with its acceptance recipe preserved.
- **[1360](../issues/archived/1360-tier2-nightly-stale-zephyr-workspace-codegen-version.md)
  is not from this campaign but was the most urgent codegen-area defect**: the
  tier-2 nightly builds against a persistent `~/.nros/workspaces/zephyr/3.7`
  whose generated trees were emitted at an older codegen version, so the
  refusal guard fires on every Zephyr fixture. The guard is right; the
  persistent workspace is the bug. It was fixed alongside this phase.

- [1306](../issues/1306-cli-build-rs-misses-worktree-git-index.md): in a
  linked worktree, the CLI source stamp never re-runs on a commit. Every
  agent hit this. The workaround is `touch packages/cli/nros-cli-core/build.rs`.
  The fix is `git rev-parse --git-path index`.
- [1307](../issues/1307-sizes-build-nested-cargo-rewrites-root-lock.md): every
  NuttX build rewrites the root `Cargo.lock`. `--locked` alone would break the
  build, and the issue lists the options.
- [1311](../issues/archived/1311-cyclonedds-sumseq-generated-c-fails-to-compile.md):
  RESOLVED 2026-09-18. It reproduced on a clean `main` checkout on the first
  attempt and was NOT a provisioning artifact: the generated descriptors were
  sources of nine targets at once, so a parallel `make` ran `idlc` up to eleven
  times concurrently into the same files. Fixed with one OBJECT-library owner
  per generated set, gated by `check-cmake-generated-source-owners`.
- [1312](../issues/archived/1312-zephyr-system-generate-passes-no-image-id.md):
  the Zephyr system-generate module passed no image id, so the tier resolver
  answered for the host. **RESOLVED** — `codegen-system --for-entry`; the shim
  names its entry package and the image that claims it supplies the target.
- [1313](../issues/1313-nros-tests-gated-absence-env-race.md): an env race
  under plain `cargo test`.
- [1314](../issues/1314-tier1-lane-contract-test-refuses-unscoped-run.md): a
  tier-1 lane-contract test failed when run directly. Establish whether it is a
  defect or a precondition before touching it.

## W3 — triage the backed-up local branches

**TRIAGED 2026-09-18.** Ten local branches carried commits with no
patch-equivalent on `main`, and were pushed to origin under
`backup/local-2026-09-11/<original-name>` before anything was deleted. Every ref
has now been checked against `main` at `fa6cf68d2`, ~420 commits after they were
taken.

**Nine of the ten are REDUNDANT.** `git cherry` marks a patch `+` when no
equivalent landed *unchanged*, which does not distinguish "never landed" from
"landed edited" — and here it was almost always the latter. What settled each
one was the issue it cites plus the code:

| ref (under `backup/local-2026-09-11/`) | verdict | evidence |
| --- | --- | --- |
| `fix/0998-sertype-freestanding` (10) | redundant | Its issues are all resolved+archived on main: 1003, 1010, 1011, and 0998's own fix landed under the ids 1014 / 1023. phase-414 and phase-416 docs exist on main. Main's 0968 doc is 597 lines against this ref's 236. |
| `backup/pre-recovery` (5) | redundant | Same 0998 / 0999 / 0968 / 1003 set; 0999 and 1003 are archived, and the sertype guard is in `nros_sertype.cpp` on main. |
| `fix/1172-tier-group-node-qualified` (4) | redundant | Issue 1172 is resolved+archived; main carries the other session's variant, merged as #741 plus its follow-up #898. |
| `backup/741-pre-rebase` (2) | redundant | Pre-rebase snapshot of the same #741 work. |
| `verify/0979-0985` (2) | redundant | 0979 and 0985 are both resolved+archived; `check-config-header-single-writer.py` is on main, and `platform_config.rs` has since moved to `packages/tooling/nros-platform-config/`. |
| `fix/0979-build-script-platform-root` (1) | redundant | Same 0979, archived. |
| `fix/0972-domain-range` (1) | redundant | Issue 0974 is resolved+archived. |
| `fix/cxx-compat-libstdcpp-passthrough` (1) | redundant | Main's `zephyr/cxx-compat/*` shims already carry the passthrough: `#if defined(__has_include_next) && __has_include_next(<atomic>)`. |
| `feature/phase-172` (1) | redundant, one cosmetic remainder | The `ZENOH_LOCATOR` half landed: all five `scripts/debug/*` read it on main. What is left is a two-line "CONFIGURE ME" comment on the stm32f4 porting reference mains. Not worth a PR; re-add it if anyone touches those files. |
| `wip/zenoh-linux-test` (1) | **LIVE — being landed** | See below. |

**The one rescue.** `wip/zenoh-linux-test` holds `a270ceb3e`, the only fix on
file for [issue 1039](../issues/1039-nuttx-stdbool-nonconforming-breaks-zenoh-pico.md)
— `status: open`, `severity: high`: NuttX's `stdbool.h` defines `true` as
`(bool)1`, which is not an integer constant, so zenoh-pico's `keyexpr` template
(included once per value) breaks every NuttX build. Main's
`nros-zpico-build/src/runner.rs` has no such helper, so the work never landed.
It is being re-derived onto current `main` on `fix/1039-nuttx-conforming-bool`,
with the premise re-validated (the NuttX header, the zenoh-pico pin, and a real
build) rather than trusted.

**Done when:** issue 1039's fix is merged, and the `backup/local-2026-09-11/*`
refs are deleted. Deleting them is a separate, explicit step: they are the only
copy of those 28 commits, and this table is the record of why nine of them do
not matter.

## Resume

```
gh pr list --state open --search "934 936 937 938"
just issues --id 1311    # and 1306, 1307, 1312, 1313, 1314
git ls-remote origin 'refs/heads/backup/local-2026-09-11/*'
```

Tooling notes for whoever runs this from an agent worktree:
- Agent worktrees start without most submodules. Initialise the ones a gate
  needs non-recursively, at their pins: cyclonedds, zenoh-pico, micro-cdr,
  micro-xrce-dds-client, play_launch (then `just setup-launch-resolve`).
- Run `just ci gate` on its own. Two parallel gate runs were killed by the
  low-memory killer on 2026-09-11.
