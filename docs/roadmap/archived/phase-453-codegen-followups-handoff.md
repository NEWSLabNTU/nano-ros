# Phase 453 — codegen campaign follow-ups: landing and handoff

**Status (2026-09-20). COMPLETE — W1, W2 and W3 all closed.** Phase 432 (RFC-0091, "one codegen producer,
many language packs") is finished. Its archive lands with PR #937. This doc
exists so the next session, human or agent, can pick up from `main` without
reading a chat log. Every item below is on origin. Nothing lives only on one
machine.

**Relates to:** [RFC-0091](../../design/0091-one-entry-codegen-producer-many-language-packs.md),
[RFC-0068](../../design/0068-language-neutral-codegen-ir.md).

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

**DONE 2026-09-20.** All eight issues are `resolved` on `main`. Two of this
phase's own filings were corrected by the work: 1311 was NOT a provisioning
artifact but a real parallel-`make` defect on `main` (another session filed the
same defect as 1367 and archived it as a duplicate of 1311), and 1039's premise
had expired, so it was closed with the measurement rather than fixed. Merged as
#1062 (1360), #1066 (1039), #1071 (1312), #1072 (1336 + 1306), #1073 (1313 +
1314), #1074 (1307).


- **DONE (2026-09-18)** —
  [1336](../../issues/archived/1336-cli-source-stamp-unwatched-in-worktree.md),
  which absorbed this item's
  [1306](../../issues/archived/1306-cli-build-rs-misses-worktree-git-index.md) as
  a duplicate: in a linked worktree the CLI source stamp never re-ran on a
  commit, and every agent hit it. Fixed with the
  `git rev-parse --git-path index` this row called for (plus
  `--path-format=absolute`), swept across six siblings — two of them live
  defects in other gates — and gated by
  `check-git-dir-layout-assumptions`. The `touch
  packages/cli/nros-cli-core/build.rs` workaround is no longer needed.
- [1307](../../issues/archived/1307-sizes-build-nested-cargo-rewrites-root-lock.md)
  (RESOLVED): every NuttX build rewrote the root `Cargo.lock`. `--locked` alone
  would have broken the build; the size probe resolves against a seeded copy in
  its own probe dir (`resolver.lockfile-path`), and
  `check-nested-cargo-lock-discipline` keeps the next nested cargo from
  bypassing the `--locked` shim the same way.
- [1311](../../issues/archived/1311-cyclonedds-sumseq-generated-c-fails-to-compile.md):
  RESOLVED 2026-09-18. It reproduced on a clean `main` checkout on the first
  attempt and was NOT a provisioning artifact: the generated descriptors were
  sources of nine targets at once, so a parallel `make` ran `idlc` up to eleven
  times concurrently into the same files. Fixed with one OBJECT-library owner
  per generated set, gated by `check-cmake-generated-source-owners`.
- [1312](../../issues/archived/1312-zephyr-system-generate-passes-no-image-id.md):
  the Zephyr system-generate module passed no image id, so the tier resolver
  answered for the host. **RESOLVED** — `codegen-system --for-entry`; the shim
  names its entry package and the image that claims it supplies the target.
- [1313](../../issues/archived/1313-nros-tests-gated-absence-env-race.md): RESOLVED —
  an env race under plain `cargo test`; the verdict's inputs are injected now.
- [1314](../../issues/archived/1314-tier1-lane-contract-test-refuses-unscoped-run.md):
  RESOLVED — it was in no lane AND asserted a premise phase-395 W19 retired;
  the direct run lacked no precondition.

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
| `wip/zenoh-linux-test` (1) | redundant | Issue 1039 is resolved+archived: the fork's patch line already carried the fix. See below. |

**The one rescue.** `wip/zenoh-linux-test` holds `a270ceb3e`, the only fix on
file for [issue 1039](../../issues/archived/1039-nuttx-stdbool-nonconforming-breaks-zenoh-pico.md)
— `severity: high`: NuttX's `stdbool.h` defines `true` as
`(bool)1`, which is not an integer constant, so zenoh-pico's `keyexpr` template
(included once per value) breaks every NuttX build. Main's
`nros-zpico-build/src/runner.rs` has no such helper, so the work never landed.
It was re-derived onto current `main` on `fix/1039-nuttx-conforming-bool`, and
re-validating the premise (the NuttX header, the zenoh-pico pin, and a real
build) rather than trusting it is what closed the issue: the fork's patch line
already carries the fix at `a1c741db`, an ancestor of the recorded pin, so the
rescued commit is REDUNDANT. Issue 1039 is resolved and archived.

**DONE 2026-09-20.** The ten `backup/local-2026-09-11/*` refs were deleted from
origin after this table recorded why nine were redundant and the tenth (1039)
was settled. Original: issue 1039's fix is merged — done, it was already on the patch
line — and the `backup/local-2026-09-11/*` refs are deleted. Deleting them is a
separate, explicit step: they are the only
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
