---
id: 1454
title: "A `play_launch_parser` bump's acceptance is PROSE — the named tests did
  not touch the parser, and the one that does reads a fixture nothing forces to
  be rebuilt"
status: resolved
type: tech-debt
area: build, testing
severity: medium
found: 2026-09-22
related: [issue-1413, issue-0381, issue-0445, issue-0466, issue-0041, issue-0683, rfc-0060, rfc-0099, phase-422]
---

## What was measured

`just/workspace.just` carried, as the instruction for what to re-run when the
`play_launch_parser` pin moves, the sentence *"the L.6 gate tests
`phase212_l6_launch_synth::*`"*. Those tests **do not exercise the parser at
all**:

* `packages/testing/nros-tests/tests/launch_synth.rs` says so in its own header
  — the pre-296 synthesis and launch-file-precedence branches moved to
  `ros-launch-resolve` and were removed, and *"This test needs only the `nros`
  CLI — no `play_launch_parser`"*.
* `packages/testing/nros-tests/tests/self_bringup.rs` records the other half:
  *"issue 0381 — the pre-296 `play_launch_parser_available()` gate is GONE"*.

So the acceptance named a suite that had been rewritten to synthesise
in-process, with the parser-availability gate deleted, and therefore could not
report anything about a parser bump in either direction. Measured rather than
inferred, **twice**: the suite was run with each parser build in turn on `PATH`
and came back 4/4 both ways — insensitive.

The acceptance that DOES bind the binary is `nav2_compat`'s
**`n11_launch_xml_ros2_compat_smoke`**
(`packages/testing/nros-tests/tests/nav2_compat.rs`). It is a build-stage
fixture: the entry package's `build.rs` drives `nros_build::generate_run_plan`
over a nav2-shaped `launch/system.launch.xml` **with `play_launch_parser` on
`PATH`**, and the test then inspects the emitted `out/run_plan.rs` and
`out/nros-system/nros-plan.json`. It has the right refusal too — if the build
fell back to the offline `Placeholder` stub it SKIPS, carrying the `// reason:`
line the stub records (issue 0683), so an absent parser cannot read as a pass.

PR #1130 corrected the sentence in `just/workspace.just`, and the corrected text
is good: it names `nav2_compat`, says the fixture must be REBUILT first, and
gives the command.

## The residue: the correction is itself prose

**Nothing asserts that the fixture was rebuilt against the new parser before a
bump lands.** The gate PR #1130 added,
`scripts/check-play-launch-parser-ref.py`, measures the relationship between the
index's `source.ref` and the `packages/cli/third-party/play_launch` gitlink —
the pin question, correctly, and only that. No check reads the fixture.

Two mechanisms were examined and neither makes a parser upgrade an edit event:

1. **cargo does not see it.** `packages/cli/nros-build/src/lib.rs` emits
   `rerun-if-changed` on the launch file, on the `system.toml` dependency, on
   the resolved model path and on the plan path, plus
   `rerun-if-env-changed=NROS_MODEL_DIR`. There is no directive naming the
   parser binary. A new parser therefore does not invalidate the build script's
   output.
2. **The freshness probe does not see it either.** The fixture resolves through
   `require_compile_check`, i.e. `require_prebuilt_binary_fresh` over the
   `.compile-ok` stamp, and the probe
   (`packages/testing/nros-tests/src/fixtures/staleness.rs`) compares the
   artifact against recorded build inputs and repo sources. The parser lives in
   the SDK store (`~/.nros/sdk/play_launch_parser/<version>/bin/`) — outside the
   repository and outside the recorded input set — so a bump moves nothing the
   probe examines and the artifact is not called stale.

The result is the museum-fixture shape CLAUDE.md warns about, in the one place
where the skip arm cannot rescue it: a fixture built with the OLD parser
contains **real codegen evidence**, not the `Placeholder` stub, so the test
neither skips nor fails. It asserts, truthfully, that some parser once produced
a correct plan — and reports that as acceptance for a parser it never ran.

That is worse than the stale-verdict class of issue 0445, where a STALE message
at least announces that nothing ran. Here the pass is indistinguishable from a
real one.

## Why it matters

A `play_launch_parser` bump is precisely the change this acceptance exists to
guard, and the capability at stake is not cosmetic. PR #1130's own measurement
of three parser revisions against the same two launch files found that on one of
them the `$(eval …)` substitution **exits 0 with the substitution unexpanded** —
a wrong node name, no diagnostic, no non-zero status. A bump that silently
regressed to that behaviour would be caught by `n11_launch_xml_ros2_compat_smoke`
only if the fixture were rebuilt, and nothing requires it to be.

The second reason is the pattern rather than the instance. This is the **sixth**
stale lockstep-or-acceptance claim found in this one neighbourhood: the index
comment beside `[tool.play_launch_parser]`, `nano-ros-sdk`'s
`build-play_launch_parser.sh`, `just/workspace.just` three times over (the
acceptance sentence, *"the version is the SUBMODULE COMMIT"*, and the lockstep
assertion), plus an agent's own restatement of one of them during the work. Five
of the six were corrected by PR #1130 by rewriting the prose; that is the right
repair for a false statement and it does not change the property that made them
false, which is that a sentence about what to re-run degrades silently while the
thing it describes moves.

Issue 1413 covers the **pin-lag half** of this component (two values naming one
component, now gated and half-closed, the remaining gap blocked on upstream
regaining a Python-capable CLI). This issue covers the **acceptance half**: even
with both pins agreeing, the test that proves the pin is good can pass on an
artifact built from the previous one.

## Fix candidates — stated, none chosen

1. **`rerun-if-changed` on the resolved parser binary.** The obvious road, and
   it has a specific hole worth recording before anyone takes it: the store is
   versioned (`.../play_launch_parser/<version>/bin/...`) and it ACCUMULATES
   (issue 0500), so a bump changes the path rather than the file. An edge
   recorded against the OLD path names a file that still exists and is
   unchanged, so it never fires — the rebuild would be triggered by the very
   thing it is supposed to detect only if the edge were re-recorded first. It
   also puts an absolute, host-dependent store path into a build fingerprint,
   which is issue 0491's neighbourhood (watch content, not a path).

2. **Put the parser's identity in the fixture's cache key.** The fixture layer
   already has `packages/testing/nros-tests/src/fixtures/cache_key.rs`, and a
   parser version recorded there makes a bump invalidate the artifact by
   construction rather than by an mtime race. Cost: the key gains an input that
   is not a repo fact, and every host without the parser has to have a defined
   answer for it.

3. **Record the parser's version in the emitted plan and assert it.** Have
   `build.rs` write the `--version` it actually invoked into the plan or the
   stamp, and have `n11_launch_xml_ros2_compat_smoke` compare it with
   `[tool.play_launch_parser].version` from the index — turning a museum pass
   into a fail or a skip. This is the only candidate that closes the hole at the
   point where the wrong answer is currently produced. Cost: it creates a new
   declared fact carrier, with the failure modes that family has (an empty
   carrier is archived issue 1429, three weeks old) and the producer/consumer
   pairing that `check-declared-fact-carriers` exists to police.

4. **Make the acceptance executable instead of prose.** One recipe that rebuilds
   the fixture and runs the test, so the bump procedure is a command rather than
   a comment — and cite the recipe from the index entry, which makes
   `check-doc-recipe-refs` responsible for it not rotting. Cheapest by far, and
   it does not make a bump that SKIPS the step fail; it only makes the correct
   step easy to run.

5. **Gate the bump directly.** Have `check-play-launch-parser-ref` — which
   already reads the index and the gitlink on the fast line — additionally
   refuse a change to `[tool.play_launch_parser].version` unless the same commit
   touches something that records the fixture rebuild. This gets the enforcement
   onto the lane that already exists, and it is the most likely to be wrong in
   an annoying way: "somebody re-ran the acceptance" is not a property of a
   diff, so any such check is a proxy and proxies in this tree have a habit of
   being satisfiable without doing the thing.

Not picked on purpose. (1) and (2) are freshness answers, (3) is a correctness
answer, (4) and (5) are process answers, and they are not alternatives to each
other in the way a single choice would imply — a real repair is plausibly (3)
plus (4).

## Acceptance for whatever is taken

It has to be demonstrated in the direction that currently passes wrongly: build
the `nav2_compat_smoke` fixture against one parser, put a DIFFERENT parser on
`PATH`, and require `n11_launch_xml_ros2_compat_smoke` to stop reporting a pass.
A green run after a correct rebuild proves nothing — that is already green
today, which is the whole defect.

## Resolution — the edge, plus an assertion, plus a correction to WHICH parser

Taken: fix-candidate **3** (record the parser's identity and assert it) as the
loud half, on top of candidate **1**'s *intent* expressed the way candidate 1's
own recorded hole demands — content, never a store path. Candidate 4 came for
free: the rebuild command is printed by the failure rather than remembered from
a comment, so `just/workspace.just` no longer has an acceptance sentence that
can go stale. Candidate 2 (the fixture cache key) and candidate 5 (gating the
bump from a diff) were not taken; the reasons are below.

### The measurement that changed the fix

Both prose answers named the wrong tool, so an edge on the obvious one would
have watched a file no build reads. Traced with `strace -f -e trace=execve`:

| build | traced `execve` | `nros-launch-resolve` | store `play_launch_parser` |
| --- | --- | --- | --- |
| `nav2_compat_smoke` (cargo-build row, full + successful) | 29,828 | 2 | **0** |
| `pure_c_workspace` (cmake-configure row) | 37,244 | 2 | **0** |

The SDK-store binary (`~/.nros/sdk/play_launch_parser/bin/`) is a standalone
CLI **no build path spawns**. What parses is the `play_launch_parser` crate
**statically linked into `nros-launch-resolve`** from the
`packages/cli/third-party/play_launch` submodule: `stage_tree` runs `nros
sync`, `run_sync` spawns the resolver by absolute path (never `$PATH` — issue
0285), and the resolved SystemModel is what the entry's `build.rs` bakes. A
static sweep agrees: no Rust, cmake or example source in the tree executes the
store binary; the only references are a `command -v` presence check, `just
doctor`, and `activate.sh`'s PATH.

So the issue's own framing — "a build-stage fixture that drives the parser
**with `play_launch_parser` on PATH**" — carried the same defect it documents,
one layer in: the clause was inherited from PR #1130's prose and had never been
measured. The fixture IS the binding acceptance; the binary it binds is the
submodule half.

### What landed

1. **One spelling for the identity** — `scripts/build/launch-resolver-identity.sh`,
   `nros_launch_resolver_identity <repo_root>`. It reports the `play_launch`
   commit the resolver compiled in (`--version`'s `NROS_PLAY_LAUNCH_SHA`),
   falling back to the binary hash for a resolver predating that field, and
   exits 1 with no output when there is no resolver so each caller spells its
   own absent-marker — the ladder `codegen-fingerprint.sh` established, not a
   second one. Sourceable AND runnable, because the Rust fixture resolver has
   to ask the same question and a second implementation is how two answers
   drift apart.

   Why that identity and not a `rerun-if-changed` on the store path: the store
   accumulates (issue 0500), so a bump changes the PATH rather than the file —
   an edge recorded against the old path names something that still exists and
   never fires, which is the hole candidate 1 recorded against itself, and it
   puts a host-dependent absolute path into a fingerprint, which is issue 0491.
   The commit is also not a NEW declared fact: `nros sync` already stamps it
   into every model it resolves (`meta.resolver`, issue 0427) and
   `model_gate::provenance_stale` already treats a change in it as staleness.
   This is that rule one layer up, on the artifact the model gets baked into.

2. **The edge** — `compile-check-signature.sh` folds the identity into
   `.inputsig` beside `tool:nros`, so `scripts/test/compile-check-stale.sh` and
   the `check-fixtures-stale` preflight report a parser bump as staleness.
   Unconditional, the same over-approximation `tool:nros` already accepts.

3. **The assertion** — `compile-check-fixtures.sh` writes
   `tool:nros-launch-resolve=<commit>` into `.compile-ok` (one helper,
   `nros_write_compile_ok`, replacing eight hand-written `date >` sites), and
   `nros_tests::fixtures::require_compile_check{,_bin}` refuses a fixture baked
   by a different parser — naming both commits and the rebuild command. This is
   `require_west_fixture`'s `tool:nros` guard (#185) applied to the lane that
   never had one, and the two now share one reader. The stamp records the
   resolver only for rows whose staging actually ran `nros sync`, because a
   false stale on an assertion is a hard failure rather than a rebuild.

   Both unknowns are "cannot judge", never "stale": a stamp with no resolver
   line (every stamp predating this, and every `cxx-syntax` row) and a host
   with no resolver. The `.inputsig` edge is what calls the first of those
   stale, which is why the fix is both halves and not either one.

### Demonstrated in the direction that used to pass wrongly

```
# fixture baked at play_launch 9a610488, then the parser is bumped:
$ git -C packages/cli/third-party/play_launch checkout --detach eed2ade5
$ just setup-launch-resolve      # play_launch eed2ade5239c…
$ cargo nextest run -p nros-tests --test nav2_compat
  FAIL [0.004s] nros-tests::nav2_compat n11_launch_xml_ros2_compat_smoke
  Compile-check fixture `nav2_compat_smoke` is STALE — it was built with a
  DIFFERENT launch parser than the one on disk now.
    baked by play_launch: 9a61048813da
    on disk now:          eed2ade5239c
  Rebuild it:  NROS_FIXTURE_ID=nav2_compat_smoke bash scripts/build/compile-check-fixtures.sh

# negative control — the stamp as the OLD builder wrote it (date only),
# same bumped parser, same tree:
$ grep -v 'tool:nros-launch-resolve=' .compile-ok > … && cargo nextest run …
  PASS [0.079s] n11_launch_xml_ros2_compat_smoke

# the edge half, same state:
$ bash scripts/test/compile-check-stale.sh "$record"
  nav2_compat_smoke (stale …/.inputsig)
```

The control is the point: the pass was indistinguishable from a real one, and
it still is for anyone who has not rebuilt — which is what `.inputsig` covers.

### Not taken, and why

* **Candidate 2 (the fixture cache key).** The key is `phase-395 W10`
  shadow-mode: it observes and records, and by construction "cannot skip
  anything, cannot serve anything, and cannot fail this resolution". Putting
  the answer there would have made it invisible until that layer goes live.
* **Candidate 5 (gate the bump from the diff).** Unchanged from the issue's own
  assessment: "somebody re-ran the acceptance" is not a property of a diff. The
  stamp makes the artifact answer for itself instead, which is the same
  guarantee without the proxy.

### Residue

`compile-check-fixtures.sh`'s cmake lane still requires `command -v
play_launch_parser` and records a lane SKIP without it, on the stated grounds
that "the C/mixed Entry templates parse launch XML via play_launch_parser" —
which the 37,244-call trace above measures as false. A host with a provisioned
resolver but no store binary therefore skips every cmake fixture for a reason
that is not true. Left alone here deliberately: changing it makes a lane RUN
where it used to skip, which is a behaviour change that wants its own
measurement rather than a ride on a fix about freshness. The sentence is MARKED
false in place with the measurement beside it, because an unmarked false claim
is what this issue is about; the two other copies of it — `nav2_compat.rs`'s
module header and `just/workspace.just` — are corrected rather than marked,
since nothing depends on them being kept.
