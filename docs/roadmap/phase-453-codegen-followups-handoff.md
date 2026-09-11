# Phase 453 — codegen campaign follow-ups: landing and handoff

**Status (2026-09-11). Handoff.** Phase 432 (RFC-0091, "one codegen producer,
many language packs") is finished. Its archive lands with PR #937. This doc
exists so the next session, human or agent, can pick up from `main` without
reading a chat log. Every item below is on origin. Nothing lives only on one
machine.

**Relates to:** [RFC-0091](../design/0091-one-entry-codegen-producer-many-language-packs.md),
[RFC-0068](../design/0068-language-neutral-codegen-ir.md).

## W1 — land the four open PRs

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

- [1306](../issues/1306-cli-build-rs-misses-worktree-git-index.md): in a
  linked worktree, the CLI source stamp never re-runs on a commit. Every
  agent hit this. The workaround is `touch packages/cli/nros-cli-core/build.rs`.
  The fix is `git rev-parse --git-path index`.
- [1307](../issues/1307-sizes-build-nested-cargo-rewrites-root-lock.md): every
  NuttX build rewrites the root `Cargo.lock`. `--locked` alone would break the
  build, and the issue lists the options.
- [1311](../issues/1311-cyclonedds-sumseq-generated-c-fails-to-compile.md): the
  `rmw-cyclonedds` `SumSeq` compile and link failure. **Reproduce on a clean
  main checkout first.** It may be a worktree-provisioning artifact.
- [1312](../issues/1312-zephyr-system-generate-passes-no-image-id.md): the Zephyr
  system-generate module passes no image id, so the tier resolver answers for
  the host.
- [1313](../issues/1313-nros-tests-gated-absence-env-race.md): an env race
  under plain `cargo test`.
- [1314](../issues/1314-tier1-lane-contract-test-refuses-unscoped-run.md): a
  tier-1 lane-contract test failed when run directly. Establish whether it is a
  defect or a precondition before touching it.

## W3 — triage the backed-up local branches

Ten local branches carried commits with no patch-equivalent on `main`. Before
anything was deleted, they were pushed to origin under
`backup/local-2026-09-11/<original-name>`. Some may have landed in edited form;
`git cherry origin/main <ref>` marks only the patches that did not land
unchanged. For each ref: land it, file what it knew, or delete it with a
one-line reason here.

| ref (under `backup/local-2026-09-11/`) | not on main | subject area |
| --- | --- | --- |
| `fix/0998-sertype-freestanding` | 10 | #0998 sertype TU, #0968/#1003 Zephyr XRCE docs, phase-414 |
| `backup/pre-recovery` | 5 | #0998, #0999 preflight, #0968/#1003 docs |
| `fix/1172-tier-group-node-qualified` | 4 | a squashed variant of #741; main carries the other session's version |
| `backup/741-pre-rebase` | 2 | #741 before its rebase |
| `verify/0979-0985` | 2 | #0979 build-script cwd, #0985 sizes-heal |
| `feature/phase-172` | 1 | 192.10 infra/debug endpoints |
| `fix/0972-domain-range` | 1 | #0972/#0974 ROS domain cap |
| `fix/0979-build-script-platform-root` | 1 | #0979 |
| `fix/cxx-compat-libstdcpp-passthrough` | 1 | Zephyr cxx-compat shims |
| `wip/zenoh-linux-test` | 1 | #1039 NuttX true/false |

**Done when:** every row has an outcome and the `backup/local-2026-09-11/*`
refs are deleted.

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
