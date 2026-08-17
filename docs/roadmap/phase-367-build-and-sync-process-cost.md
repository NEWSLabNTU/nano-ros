# Phase 367 — what a build actually spends, measured per layer

**Status (2026-08-17).** W1, W2, W4 and W5 LANDED; **W3 is the one open wave**,
and it is a design step rather than a cut. The sync loop is 7.0 s -> 1.4 s. Opened to give the `nros sync` / fixture-build cost work one
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

## W2 — a diagnostic probe was 42 % of a warm sync (LANDED)

W2 was written as "the remaining 741 failed `execve` are a PATH scan", with a
warning that a large count attached to a small cost is the shape that looks like
progress and is not. **That warning was right about the method and wrong about
the answer, in both directions**, which is worth recording in full.

**Step 1 — the hypothesis, tested.** Running the same sync with
`PATH=/usr/bin:/bin` (byte-identical output, same exit) took **0.060 s against
0.156 s**. That looked like the PATH scan and justified cutting it.

**Step 2 — the fix, measured, and it bought nothing.** Resolving `git` once
through a `OnceLock` removed 163 failed lookups (`execve` 260 → 97, git
lookups → 0) and moved wall clock by **~2 ms**. The count was real; the cost
was not. Kept anyway — it is strictly fewer syscalls with no downside — but it
is not why this wave landed.

**Step 3 — re-profile, and the real cause was structural.** With the short PATH,
`wait4` was **9 calls against 73**. A short PATH does not make lookups faster,
it makes tools *not be found*, so probes skip their subprocesses entirely. The
saving was never in the scan.

The probe is `warn_if_cargo_predates_config_include`: a `cargo --version` run
whose only effect is to print a warning if cargo predates 1.93 (#272). Through
this repo's `scripts/bin/cargo` PATH shim it fans out into
`env` → `bash` → `dirname` → `grep` → the real cargo → `rustc -vV`. Isolated:

```
current (shim on PATH)                0.160 s
CARGO=<real cargo>, shim skipped      0.115 s
CARGO=/bin/false, probe fails fast    0.093 s
```

**42 % of a warm sync, spent deciding whether to print a warning** — and a build
runs ~101 syncs, so ~6.8 s per fixture build.

The warning is for an EXTERNAL consumer on an old pinned toolchain. An in-repo
workspace cannot be that case: its toolchain is pinned by the checkout, and the
build that follows uses the same shim-wrapped cargo. `find_monorepo_root` is
exactly that predicate.

```
                   before W1   after W1   after W2
wall clock (mixed)   0.156 s    0.153 s    0.094 s
wait4 calls              88         73          8
execve                  770        260         38
22-workspace loop       4.2 s       —         2.7 s
regenerate-bindings     9.4 s       —         7.1 s
```

**Acceptance — the diagnostic still fires where it is for.** The predicate is
extracted as `cargo_version_warning_applies` precisely so both arms are
testable: a predicate buried in a side-effect-only function can only be checked
by watching for a warning that may legitimately not appear, which is no check at
all. In-repo → skipped, nested-in-repo → skipped, out-of-tree tempdir → applies.
Mutation-tested: making it return `false` everywhere fails the out-of-tree arm.

## W2b — the rest of the PATH scan (open, and probably not worth it)


`git` is resolved once now, but `bash`, `rustc` and `cargo` are still looked up
by name (33, 28 and 18 failed lookups respectively at the last count). After W2
removed the probe that spawned most of them, `execve` is 38 calls total, so
there is little left to win here.

**Recorded as measured-and-declined rather than open work.** W2's step 2 is the
evidence: removing 163 failed lookups moved wall clock by ~2 ms.

## Where it ended up

| | original | now |
| --- | --- | --- |
| `sync examples/workspaces/mixed` | 1.24 s | **0.068 s** |
| 22-workspace sync loop | 7.0 s | **1.4 s** |
| `regenerate-bindings.sh` | 12.8 s | **6.5 s** |
| sync invocations per build | 185 | **101** |
| `wait4` per sync | 88 calls | **8** |
| `execve` per sync | 770 | **38** |

## W3 — the last cross-driver sync repeats need a freshness stamp (OPEN)

Issue 0649 took the census from 185 invocations to 101 for 69 targets. The
remaining 32 are all 2–4× and ACROSS drivers — `regenerate-bindings.sh`, both
fixture pre-passes, `just/native.just` — each legitimately ensuring its own
precondition in a separate process.

Removing them needs a cross-process stamp: "this workspace's msg inputs have not
changed since the last sync". **This is a design step, not a hoist.** A wrong
digest leaves a stale `generated/` compiling against the wrong shape, which is
what phase-214.J was about; today's `nros_codegen_stamp_check_or_wipe` gates the
WIPE, not the sync. Wave 3 is the digest and its gate, not the plumbing.

## W4 — a configure failure stops taking the whole workspace down (LANDED)

The probe's batch configure was all-or-nothing: one component's `find_package`
error aborted it, so EVERY component in the workspace lost its sidecar. That
contradicted this driver's own contract, stated for the BUILD step a few lines
below — *"ONE unprobeable component degrades to the sidecar-less path by NAME
rather than taking the whole workspace with it"* — which held for builds and not
for configures, where the code asserted the opposite: *"A configure failure IS
fatal: it means the project itself is malformed."*

`run_probes` now drops the components CMake NAMED and retries with the rest,
using CMake's own attribution (`CMake Error at <dir>/CMakeLists.txt:N`, matched
against `package_dir`). An error naming nothing keeps the old fatal behaviour —
a project really can be malformed, and picking a victim would be worse. Dropped
components are reported BY NAME as failures, never omitted: a component that
vanishes from the outcome list is a sidecar nobody knows is missing.

**The underlying cause turned out NOT to be an ordering constraint, and is now
fixed — issue 0662.** It was read here as one, and the question "isn't this a
circular dependency?" is what disproved it: `custom_msgs` is a verbatim upstream
msg package needing only the ROS install, so nothing it needs comes from sync.
The workspace's own CMakeLists already resolves it with
`set(NROS_INTERFACE_SEARCH_PATH "<ws>/src")` and the compat layer's Find-stub
emitter; the probe project simply never set it. **0 of 16 components probed
became 16 of 16.** What follows is the original reading, kept because the retry
below was built on it and remains correct:
`examples/workspaces/features`' 16 C/C++ components all `find_package(custom_msgs)`,
a workspace-local interface package built by the workspace's own CMake build,
which runs AFTER sync because sync generates what that build consumes. At sync
time there is no config file and no install prefix, verified. So they are
unprobeable by construction, and all 16 fall back to the SystemModel bound.

In that workspace the retry changes no outcome — the loop drains 16 → 12 → 11 →
5 → 4 → 1 and every component fails on the same missing package. **That is a
property of `features`, not of the mechanism**, and it is why 0662 is filed
rather than closed. Cost, stated: the cold path is ~5x more configures (5.5 s
against roughly one configure), paid once per source change because issue 0641's
negative marker absorbs the repeat. The trade is made on the contract, not on
speed — a workspace with one broken component and sixteen good ones now gets
sixteen sidecars instead of none.

## W5 — the build drivers stop writing an index nobody reads (LANDED)

The question was whether to CACHE the underlay index. The answer turned out to
be simpler: in the build path, nothing reads it.

`nros sync` writes `<ws>/build/nros/providers.json`. cmake keeps its own index
at `${CMAKE_BINARY_DIR}/nros-providers.json` and, as `NanoRosProviders.cmake`
says, reads it *"THROUGH the CLI, never parsed here"* — and no caller points
`nano_ros_load_providers(INDEX …)` at the sync-written path. It exists for later
interactive commands.

So the three build drivers pass `--no-provider-index` (issue 0646's flag):
`regenerate-bindings.sh`, and the two fixture pre-passes from issue 0649. Zero
risk — the file has no reader here — and it removes the underlay scan, which
after W1/W2 was **28 % of a warm sync** (0.095 s → 0.068 s).

Caching was NOT adopted, and that is the decision: a cached index has a validity
problem (a stale one hides a newly added board) and would buy nothing the flag
does not, for the population that actually pays.

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
