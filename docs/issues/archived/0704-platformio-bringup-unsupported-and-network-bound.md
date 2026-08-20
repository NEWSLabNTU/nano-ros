---
id: 704
title: "The PlatformIO bringup test runs `pio run`, which fetches its platform package from the network and blows the 60 s per-test budget — and PlatformIO is not a supported integration right now"
status: resolved
type: tech-debt
area: testing, integrations
related: [issue-0700, issue-0584]
---

## Symptom

The last real failure in tier 2, on a host where `pio` is installed:

```
TERMINATING [> 60.000s] nros-tests::cli_bringup_platformio platformio_zephyr_framework_2_component_bringup_builds
    TIMEOUT [  60.002s] (1501/1654)
```

Not an assertion failure — it hits the per-test limit to the millisecond.

**Measured after the fact, and it changes the shape:** with the package already
cached, the same test PASSES in **3.4 s**
(`NROS_ENABLE_PLATFORMIO=1 cargo nextest run --test cli_bringup_platformio`
-> `1 passed`). So this is a COLD-CACHE failure, not a permanent one: the first
run on any machine (or any CI container with a fresh PlatformIO home) pays a
package download that does not fit the budget, and every run after it is fast.
That is worse than a permanent red for a gate, not better — it passes locally
for whoever already ran it and fails for everyone else, which is how it reached
tier 2 unnoticed.

## Why

`cli_bringup_platformio.rs` asserts the adapter surface (repo-root
`library.json`, the `integrations/platformio/nros_codegen.py` pre-build hook)
and then does the expensive part:

```rust
let out = Command::new(&bin).args(["run", "-e", "native"]).current_dir(&pio_app).output()
```

`pio run` resolves and DOWNLOADS its platform package on first use. The test
already knows this can fail and carries offline markers
("Could not find the package", "PackageManagerError") to skip cleanly — but the
60 s nextest budget kills the process before that handling is ever reached, so a
condition the test was written to tolerate is reported as a hard timeout.

Selection makes it worse: `test-all`'s `env_exclude` drops this suite only when
NEITHER `pio` nor `platformio` is on PATH. Installing the CLI — which a
developer may do for unrelated reasons — opts the machine into a
network-dependent 60 s test in the per-change tier.

## Decision (2026-08-20): opt-in, because PlatformIO is not supported at present

PlatformIO is not a maintained integration right now. A test whose subject is
unsupported should not be able to red the per-change gate, and the alternatives
are worse:

* **Raise the timeout.** Would work on a warm cache and still fails cold or
  offline — it makes the gate depend on whether the machine has run this before,
  which is exactly the property that made this hard to see.
* **Deselect it silently** (extend `env_exclude` to drop it unconditionally).
  Cheap, but the suite would vanish with nothing in the run saying why — the
  "test that could not fail" shape this tree keeps removing.

So the suite is **opt-in**: it `skip!`s with a reason naming this issue unless
`NROS_ENABLE_PLATFORMIO=1` is set. The skip is VISIBLE in the run
(`[SKIPPED]`, counted in the skip budget), the reason says both facts — not
supported, and network-bound — and the escape hatch keeps it one env var away
for anyone working on the adapter.

The adapter-surface assertions above the `pio run` (library.json, the pre-build
script) are cheap and offline, and they go behind the same gate rather than
being split out: what they check is only meaningful if someone is maintaining
the integration, which is precisely what this issue records nobody is.

## What would close this

Either PlatformIO becomes a supported integration again — in which case the
`pio run` needs a fixture built in the BUILD stage rather than at run time
(CLAUDE.md: "No compilation inside tests"), which is the real reason it is slow
— or the integration and its test are deleted. "Present but untested and
opt-in" is a holding position, not a destination.

## Follow-up (2026-08-20) — the SELECTOR still matched the old decision

The opt-in landed in the test; the deselection predicate in `test-all` did not
move with it. It read "is `pio` on PATH", which is the question that mattered
before PlatformIO became unsupported, and it was wrong in both directions:

* **On a host WITH `pio`** — exactly the host this issue was filed from — the
  suite was SELECTED, so the new `skip!` surfaced as a red console FAIL. The
  paragraph directly above that predicate in the `justfile` exists to prevent
  that specific "non-bug failure", and this suite was the one case it no longer
  covered.
* **With `NROS_ENABLE_PLATFORMIO=1` on a host WITHOUT `pio`** the suite was
  deselected regardless, so the documented escape hatch did nothing. Anyone
  working on the adapter got silence instead of the actionable
  "pio CLI not available — run `just platformio setup`".

The predicate is now the opt-in itself. The runtime `skip!` stays — it is what
says WHY, and names this issue, to anyone who runs the binary directly.

Verified: with the var unset the exclusion is emitted and `cargo nextest list
-E 'not binary(cli_bringup_platformio)'` selects nothing; with it set the
exclusion is absent and the suite runs.

**What remains is only the decision in "What would close this" above** — support
the integration (with `pio run` moved to the BUILD stage) or delete it. The
budget failure, the cold-cache dependence, and the selection bug are all fixed.

> The section above was written while the integration was still opt-in. The
> owner then decided not to support PlatformIO at all, so the selector it
> repaired is gone with the suite it selected — kept here because it records
> what the predicate was actually asking, which is the thing to get right
> again if the integration ever returns.

## Closed 2026-08-20 — the integration's TEST SURFACE is deleted, the adapter is kept

The owner's call: PlatformIO is not being supported at present, so take the
second of the two closing conditions this issue named rather than the holding
position it settled on a few hours earlier.

The opt-in gate was the right call while the question was open. It is the wrong
one now: `NROS_ENABLE_PLATFORMIO=1` guards a suite nobody is going to set it
for, which is a test that cannot fail wearing a flag — the shape this tree keeps
removing, one indirection out.

**Deleted**, because each of these asserts that somebody is maintaining the
integration:

* `packages/testing/nros-tests/tests/cli_bringup_platformio.rs` — the suite,
  adapter-surface assertions included. Those are cheap and offline, but as this
  issue already recorded, what they check is only meaningful if the integration
  is maintained.
* `packages/testing/nros-tests/fixtures/multi_pkg_workspace_platformio/` — its
  workspace fixture, referenced by nothing else.
* `just/platformio.just` and every orchestrator entry (`just setup platformio`,
  the `run platformio` tier row, `just platformio clean`, the module import).
  The `pio`-presence `env_exclude` went with the suite it deselected.

**Kept**, and this is the "parts for future work" half:

* `integrations/platformio/{README.md,nros_codegen.py}` and the repo-root
  `library.json` — the whole adapter, and DATA rather than a build path.
  Nothing in `just ci` reaches them, so they cost nothing and are exactly what
  someone resuming this would otherwise re-derive. The README now leads with an
  unsupported banner saying so.
* The CLI and board paths — `[deploy.<target>].framework` pass-through,
  `nros-board-esp32-qemu`'s descriptor. Live code shared with supported
  targets; removing it would break a board over an unrelated decision.

The schema test `accepts_platformio_framework_field` stays and still passes: it
pins a pass-through field and never needed the fixture. Its doc comment no
longer points at a deleted path.

## What would reopen this

PlatformIO becoming supported — at which point the `pio run` must be a
BUILD-stage fixture rather than run-time compilation (CLAUDE.md: "No compilation
inside tests"), which is the actual reason the old suite blew a 60 s budget on a
cold package cache. That constraint is recorded above and is the first thing to
read when picking it up.

