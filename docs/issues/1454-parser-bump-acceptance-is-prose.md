---
id: 1454
title: "A `play_launch_parser` bump's acceptance is PROSE — the named tests did
  not touch the parser, and the one that does reads a fixture nothing forces to
  be rebuilt"
status: open
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
