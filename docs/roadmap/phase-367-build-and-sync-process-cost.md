# Phase 367 — what a build actually spends, measured per layer

**Status (2026-08-17).** W1 LANDED. W2–W5 open, each with its measurement
already taken. Opened to give the `nros sync` / fixture-build cost work one
owner instead of five issues that kept rediscovering each other.

**Owns:** the per-invocation cost of `nros sync`, the number of times a build
invokes it, and the subprocess/IO profile underneath. Not build PARALLELISM
(phase-353 W3) and not artifact reuse (issue 0446).

**Related:** issues 0641, 0645, 0646, 0649 (all resolved, all this thread),
0604 (cold-leaf attribution), 0648 (cargo's global package-cache lock),
0446 (artifact reuse), 0200 (the timing campaign this does NOT resume).

## The rule this phase runs on

**Profile, then cut; re-profile, then stop.** Every step below moved a number
that was measured before and after on the same head with the same workload, and
two candidate optimisations were REJECTED on measurement after being written.
This host produces 50–695 s for provably identical work (issue 0509), so
wall-clock alone decides nothing: the load-bearing figures here are syscall
counts and invocation counts, which are deterministic.

## Where the cost was, in the order it was found

Each layer was invisible until the one above it was removed — which is the
argument for re-profiling rather than predicting.

| # | layer | finding | issue |
| --- | --- | --- | --- |
| 1 | a probe that could not succeed | a Rust package read as C++ ran a CMake probe (Corrosion and all) that failed every sync, uncached | 0641 |
| 2 | the directory walk | `PRUNED_DIRS` matched `build`/`target` EXACTLY, so every `build-*` root was walked: 3923 of 7113 dirs | 0645 |
| 3 | the walk's SCOPE | 1570 of the remaining 1590 dirs were the nano-ros underlay, rescanned per workspace | 0646 |
| 4 | how often sync runs | per fixture ROW, not per workspace: 185 invocations for 69 targets, one dir synced 22× | 0649 |
| 5 | subprocesses | 20 of ~29 spawns were the CLI source stamp, one git per closure dir | W1 below |

## W1 — the source stamp asks git once, not eighteen times (LANDED)

`source_stamp` has three consumers that each looped `cli_source_dirs()` and
spawned one `git` per directory — `ls-files`, `ls-files -s`, and
`diff --name-only`. Nine directories, three loops: **20 of the ~29 subprocesses
a warm `nros sync` spawns were this one file**, at a measured ~2.8 ms per spawn
with `wait4` at 54 % of the run.

git takes multiple pathspecs after `--` and unions the results, which is exactly
what the loops were doing by concatenation. `git_over_closure` passes the whole
closure in one invocation.

Measured on `examples/workspaces/mixed`, same head, same workload:

```
                    before      after
total syscall time  0.458 s     0.315 s     -31 %
wait4               0.249 s     0.158 s     (88 -> 73 calls)
execve                770         260       (31 -> 16 successful)
git spawns             ~19          5
```

**Acceptance — behaviour identical, not merely faster.** The stamp is
order-dependent (it hashes lines in sequence), so batching had to be checked
against issue 0627's own table rather than assumed:

| edit | expected | got |
| --- | --- | --- |
| `nros-node` (NOT in the closure) | FRESH | FRESH |
| `nros-rmw` (IS in the closure) | STALE | STALE |
| none | FRESH | FRESH |

Plus `nros source-stamp` agreeing with its baked value after a rebuild — the
thing a wrong line order would have broken permanently.

## W2 — the remaining 741 failed `execve` are a PATH scan

`git` is invoked by NAME, and this host's PATH has ~40 entries, so each
invocation costs ~40 failed `execve` before the real one. Before W1 that was
741 of 772; after W1 it is ~244 of 260.

**Measure first:** `execve` is ~1.5 % of syscall time, so this is a large COUNT
attached to a small cost. Resolve `git` once into a `OnceLock` only if a
measurement says the count is worth it — a 741-call finding that saves 9 ms is
exactly the shape that looks like progress and is not.

## W3 — the last cross-driver sync repeats need a freshness stamp

Issue 0649 took the census from 185 invocations to 101 for 69 targets. The
remaining 32 are all 2–4× and ACROSS drivers — `regenerate-bindings.sh`, both
fixture pre-passes, `just/native.just` — each legitimately ensuring its own
precondition in a separate process.

Removing them needs a cross-process stamp: "this workspace's msg inputs have not
changed since the last sync". **This is a design step, not a hoist.** A wrong
digest leaves a stale `generated/` compiling against the wrong shape, which is
what phase-214.J was about; today's `nros_codegen_stamp_check_or_wipe` gates the
WIPE, not the sync. Wave 3 is the digest and its gate, not the plumbing.

## W4 — `nros sync` still spawns a cmake metadata probe

For C/C++ components with no current sidecar, sync configures and builds a CMake
project. Issue 0641 made the FAILING case cheap (a negative cache keyed on
`source_digest + NROS_CLI_SOURCE_STAMP`); the succeeding case still pays a full
configure. Two threads live under this and neither is measured:

* `examples/workspaces/features`' probe project fails to CONFIGURE — 17
  components silently fall back to the SystemModel bound. Cheap now, still
  wrong.
* the probe's configure log says `Corrosion not provisioned — fetching v0.6.1
  from git`, i.e. a network fetch inside a sync.

## W5 — decide whether the underlay index is cached

Issue 0646 added colcon-style scope flags (`--base-paths`, `--nano-ros-root`,
`--no-provider-index`) so a caller that knows the tree can decline the underlay
scan. `--no-provider-index` takes the walk to ZERO directories and saves ~10 %
of a sync, because after W1–W3 the scan is no longer the bottleneck.

The alternative — a shared, cached underlay index — was deliberately NOT taken:
it changes provider-resolution freshness, and a stale index hides a newly added
board. W5 is that decision, with the flags as the fallback if the answer is no.

## Rejected on measurement, recorded so they are not retried

**Enumerating packages via `git ls-files` instead of walking** (issue 0646).
Implemented, with an equivalence test that immediately earned its keep — git has
no notion of ancestry, so it found the fixture packages under
`nros-rmw-cyclonedds/tests/types/` that the walk's stop-at-a-package rule hides.
That was fixable and all 147 tests passed. It was then **slower**: `statx`
12,080 → 16,302, the 22-workspace loop 3.6 s → 4.2 s. Re-applying the walk's
ancestry rules per candidate costs more than the walk it replaces.

**Narrowing the underlay scan to the three subtrees that hold providers.**
Would remove almost all of layer 3's cost and is barred by
`packages/cli/CLAUDE.md`: *"`nros` is a generic tool — it must not learn the
nano-ros directory layout."* A caller may know it, which is why W5 is a flag
rather than a constant.

## Not in scope

Wall-clock A/B of whole builds (issue 0200 / 0509 — this host cannot answer it),
build parallelism (phase-353 W3), and artifact reuse across leaf target dirs
(issue 0446).
