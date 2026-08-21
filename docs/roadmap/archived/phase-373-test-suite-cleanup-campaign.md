# Phase 373 — Test-suite cleanup: delete what cannot fail, consolidate what was copied

**Status: COMPLETE (archived 2026-08-21).** W0–W5 all landed. Acceptance verified
by measurement, not by reading the work items back:

| criterion | result |
| --- | --- |
| no live nextest `test()` filter matches zero tests (W1) | 0 |
| one repo-root helper (W2) | 0 local declarations under `tests/` |
| `check-no-vacuous-tests` + `check-nextest-binary-filters` green and self-testing | 10 + 5 self-test cases OK |
| no test whose body only prints | OK across 262 test files |

One caveat on that last row worth carrying forward: a naive `grep` for
`fn workspace_root()` still returns a hit, and it is the GATE'S OWN doc comment
quoting the pattern it forbids. The gate trims and matches `fn …` at line start,
so it is right and the grep is wrong. That same comment-vs-code confusion
produced three false findings during this phase — the `dds_api` "dead" nextest
filters, the `nuttx_riscv`/`zephyr_rust_lifecycle` "inert" ones, and this. Any
audit of this tree should parse or anchor, never grep bare.

**Summary.** W0 (`a660be83f`) removed 23
tests that could not fail and gated both shapes that admitted them. W1 restored
the issue-#141 resource protection that phase-329's consolidation had silently
switched off. W2 collapsed 23 repo-root helpers into the one that already
existed. W3 shared the three helpers that were genuinely shared — and found a
latent race doing it. W4 folded the two true duplicate pairs. W5 concluded that
the "runtime-compile test" is a labelled exception, not a defect: my own survey
item was wrong.

**Follows:** phase-329 (test taxonomy completion, archived 2026-08-05) — this is
the residue that pass explicitly left, plus the classes it could not see because
they hide in tests that PASS. **Related:** issue 0743 (filed and resolved in W0),
RFC-0051 (matrix as the test-intent SSoT).

## Problem

Phase 329 dispositioned test FILES by asking "is this a per-cell duplicate of a
matrix workload". That question cannot find a test that duplicates nothing and
asserts nothing — it reads as coverage, it is green, and no fold rule touches it.
Two shapes hid there:

- **A test that cannot fail.** 17 tests read `is_*_available()` probes and printed
  them. Each reported PASS on exactly the host it was meant to warn about; one said
  so in its own last line ("These are informational - don't fail if Zephyr isn't
  set up"). Two were literal cross-file duplicates.
- **A test subsumed by a stronger one.** A boot check under nine delivery cells
  that each boot a real image; five FreeRTOS boot tests over the same binaries the
  matrix cells already boot.

Both are worse than dead weight: they spend QEMU wall-clock on every sweep and
raise the pass count with results carrying no information.

The survey also found the OTHER direction of phase-329's own consolidation cost.
Folding 15 per-cell tests into one test does not just delete files — it deletes
the NAMES that per-cell nextest overrides selected on, and those overrides go
silently inert. Phase 329 knew this for five of them; W1 is a sixth that was
missed, and it is load-bearing.

## Work items

### W0 — remove the tests that cannot fail; gate both shapes (LANDED `a660be83f`)

- [x] 17 vacuous probes deleted across 10 files; `threadx_linux.rs` was nothing
      but one and is gone; `nuttx_qemu.rs` had zero tests left afterwards (118
      lines of comments + dead imports) and is gone too.
- [x] `test_nuttx_kernel_boots` deleted — subsumed by nine `rtos_e2e`
      `Platform::Nuttx` cells that boot a real nros kernel and assert DELIVERY.
- [x] 5 of 6 FreeRTOS `_entry_boots_and_connects` deleted. They booted the SAME
      FILES as the `Freertos × Rust` cells: `build_rust_example` discards its own
      `binary_name` argument, so `require_entry_binary("talker")` and
      `build_freertos_talker()` are one path. `talker` keeps the `run_entry`
      banner proof — a board-driver property, printed before the user closure, so
      one boot proves it.
- [x] `check-no-vacuous-tests` — a test body whose only effects are PRINTS. Keyed
      on that, NOT on "has no `assert!`": the naive rule flags ~40 correct tests
      that delegate to an asserting helper. 17/17 on the pre-cleanup tree, 0 after,
      across 263 files.
- [x] `check-nextest-binary-filters` — a `binary()` naming a deleted target.
- [x] Lane recipes repointed (`just nuttx test`, `just threadx-linux test`
      /`test-all`) via a new optional `-E` filter on `_nextest-platform`.
- [x] Issue 0743 filed and fixed: `nuttx_kernel_path_for(NuttxArch)` reads
      `e_machine` from the ELF header instead of asking `.exists()`.

496 → 473 test fns.

**Two things W0 proves that the later items depend on.** First, a deleted test
target strands consumers in three registers — a `just` recipe (loud, caught by
`check-just-recipe-refs`), a nextest `binary()` (FATAL: nextest refuses to parse
the config, killing every nextest run in the repo), and a nextest `test()`
(SILENT: the override just stops applying). Second, `just check` runs no nextest,
so the fatal one sat behind a green `check` — which is why W1 needs its own
verification and cannot ride on `just check` going green.

### W1 — the `zephyr-qos-port` resource group is off (DEFECT) — **LANDED 2026-08-21**

`test(zephyr_rust_qos)` matches ZERO tests: `entry_e2e` has had exactly one test,
`entry_matrix`, since phase-329 W1 folded its 15 cells in. So this override
selects only its second disjunct:

```toml
filter = "(binary(entry_e2e) and test(zephyr_rust_qos)) or binary(qos_zephyr_ros2_interop_e2e)"
test-group = "zephyr-qos-port"   # max-threads = 1
```

`entry_e2e` therefore sits in `matrix-consumers-serial` while
`qos_zephyr_ros2_interop_e2e` sits alone in `zephyr-qos-port`. **Different groups
do not serialize against each other**, and the two share one baked image and its
baked router port — which is the whole reason issue #141 created the group. The
protection is off and the flake it prevents is live.

It cannot be repaired in place: nextest allows ONE group per test, so
`entry_matrix` cannot be in both. The file already prescribes the fix for exactly
this situation — *"If it flakes that way, move the per-platform partner into this
group too."*

- [x] Moved `qos_zephyr_ros2_interop_e2e` into `matrix-consumers-serial`; retired
      `zephyr-qos-port` and its now-single-use override.
- [x] Swept every group override. Only this one was inert; no test-group is
      defined-but-unused, and every other `test()` name resolves.
- [x] Verified by RUNNING nextest.

**Second gate, `check-nextest-test-filters` (added after the fact).** The sibling
`check-nextest-binary-filters` deliberately checks `binary()` only, and says why:
`test()` names are rstest-generated cases that appear literally nowhere in the
sources, so deriving them means compiling the workspace — "far too heavy for
`just check`". Its own note calls the resulting gap real and already bitten, and
W1 IS that gap: the inert predicate was a `test()`, not a `binary()`.

So the same check runs where the cost is already paid — `test-all`, which has the
binaries — reading `cargo nextest list --message-format json` and asserting every
`test()`/`binary()` predicate matches at least one real test. Falsified against
this very defect: reintroducing `test(zephyr_rust_qos)` makes it name that line.
The two gates split by cost, not by scope: static and cheap on the fast line,
derived and exact in the lane that compiles.

Two traps inside it, recorded because each made it pass while checking nothing:
`cargo nextest list`'s human output is a flat `<binary-name> <test-path>` per
line, not an indented tree (parsing it as one reported all 73 predicates dead),
and `--all-features` cannot be used on this workspace at all — `c-stub-test` and
`posix-c-port` both define the canonical `nros_platform_*` symbols and the build
script `compile_error!`s on the pair. It refuses to pass on an empty list.

**ORDERING IS LOAD-BEARING, and the obvious fix was wrong.** Adding
`binary(qos_zephyr_ros2_interop_e2e)` to the `matrix-consumers-serial` override
where that group is defined did NOTHING: nextest applies the FIRST matching
override per setting, and `binary(~zephyr)` — a SUBSTRING match, ~30 entries
earlier — claims it for `qemu-emulated` first. The retired override had been
winning only by sitting near the top of the file. So the replacement override
must stay at that position, and the fix is only visible with

    cargo nextest show-config test-groups

which prints RESOLVED membership. The config parsing cleanly is not evidence
that a group applies — the broken version parsed fine for months. Verified: both
`entry_e2e::entry_matrix` and `qos_zephyr_ros2_interop_e2e`'s two tests now
resolve into `matrix-consumers-serial` (max threads = 1).

**Acceptance:** met — no live `test()` filter matches zero tests, and the two
port sharers are in one max-threads=1 group by resolved membership.

### W2 — one repo-root helper, not 23

`nros_tests::project_root()` already exists. Beside it the tests carry **23** more
declarations under three names (`workspace_root` ×11, `repo_root` ×6,
`project_root` ×6):

- 17 hand reimplementations (`ancestors().nth(3)`, chained `.parent().unwrap()`)
- 4 trivial aliases (`fn workspace_root() -> PathBuf { nros_tests::project_root() }`)
- 2 `canonicalize()` variants — **not equivalent**: they resolve symlinks, so they
  disagree with the other 21 under a symlinked checkout

- [x] All 23 deleted; call sites point at `nros_tests::project_root()`.
- [x] The symlink question is settled by deletion — both `canonicalize()` variants
      are gone, so no test resolves symlinks and none disagrees. Neither had
      CHOSEN to: canonicalising fell out of spelling the path `../../..`, which
      has to be normalised to be usable. If a test ever genuinely needs a
      canonical root, that belongs in the lib helper for everyone, not in one file.
- [x] Gated by `tests/repo_root_is_unified.rs`, which is negative-tested: a
      violating helper makes it FAIL naming file and line, and it passes again on
      revert. (A first attempt at that proof was itself invalid — an UNUSED local
      helper is a dead-code compile error under `-D warnings` before the gate can
      run, so the violation has to be one that compiles.)

**Scope, stated rather than papered over:** this covers
`packages/testing/nros-tests/tests/` only. Four sibling helpers live in
`nros-cli-core`, `rosidl-codegen` and `nros-rmw-cyclonedds`, which do not depend
on `nros-tests` and should not grow a dependency on a heavy test-support crate to
reach one path function. A shared helper for them belongs in a smaller crate; the
gate widens when that exists.

**Acceptance:** met — one definition; zero local declarations under `tests/`;
clippy clean; the 45 host-side tests over the rewritten files pass.

### W3 — share what is actually shared (LANDED)

The survey counted helpers by NAME and over-counted. Checking the bodies first
changed the answer, and that check is the work item:

- [x] **`lang_str` (8 files) — byte-identical.** Moved onto the type as
      `Lang::as_str()`. Deliberately not `Display`: the sibling `Rmw` mapping is
      genuinely ambiguous, and blessing one axis while its neighbour keeps
      per-consumer spellings would imply an agreement that does not exist.
- [x] **`spawn_listener` (7 files) — two behaviours, the common one RACY.**
      Replaced by `fixtures::spawn_int32_sink(topic, locator)`. See below.
- [x] **`fixture_dir` (13 spellings) — the NAME differs per test, but the
      fixture ROOT is one fact this crate owns.** Replaced by
      `fixtures::fixture_dir(name)`.

**NOT unified, with reasons — these were same-name, different-function:**

- **`exec_for` (6 files)** — different signature AND a different local `Exec`
  struct in every file (`resolver`/`entry`/`robot1`/`label`…). Per-consumer
  dispatch, correctly local.
- **`wl_str`** — a PARTIAL per-consumer label map with `_ => "?"` catch-alls;
  each consumer names only the workloads it runs. Not one function.
- **`rmw_str`** — `Cyclonedds` is `"cyclone"` in the native example consumers
  and `"cyclonedds"` in `zephyr.rs`. Each file is self-consistent (both sides of
  its comparison use its own spelling), so this is a vocabulary difference and
  not a bug — but unifying it blindly WOULD have been one.

**The race W3 found.** Four of the seven `spawn_listener` copies waited on the
literal `"Listener"`. The int32-sink's banner does contain that word — the source
even says the helpers key off it — but the banner is the FIRST line it prints,
before `nros::init`, before `Executor::open`, ~25 lines before
`subscription(...).build(...)`. So those tests resumed as soon as the process
emitted any log line and then published into a session that might have no
subscriber. `param_live_read_e2e` carried the comment "Subscription must be live
before the talker publishes" directly above the wait that did not ensure it. The
shared helper keys on `output::INT32_SINK_READY_MARKER` (`"Waiting for Int32"`),
which is printed after the subscription exists.

This is exactly what the repo rule about greping `nros_tests::output::*`
constants protects against: a literal keeps matching the wrong line forever and
nothing points at the mismatch.

### W4 — fold the true duplicate pairs (LANDED)

The survey named four "obvious families". Reading them, two were duplicates and
two were not — the difference being whether the files assert the same thing:

- [x] **`{nuttx,threadx_linux}_entry_build` → `entry_build.rs`.** Same six roles,
      same loop, same "ELF exists and is non-empty" assertion; only the
      availability probes and the resolver differed. Folded to one `#[rstest]`
      over the platform, keeping each platform's own skip message (they name
      different setup commands, and a merged one would be less useful than
      either).
- [x] **`{c,cpp}_parameters` → `parameters_roundtrip.rs`.** Identical runner —
      spawn prebuilt example, require exit 0, grep stdout — with the expected
      lines as case data.
- [x] **NOT folded: `cli_bringup_{esp_idf,zephyr,nuttx,px4}`.** Same name prefix,
      different assertions: esp_idf asserts a prebuilt ELF exists; zephyr asserts
      baked `system_config.{h,cmake}` AND boots native_sim AND greps a shim
      banner. Per phase-329's rule these are one-offs to keep, not duplicates to
      fold.
- [x] **Consumers moved with the fold**, which is most of the risk: a
      `test(cpp_parameters_roundtrip)` filter in `.config/nextest.toml` (which
      would have gone SILENTLY inert — the gate cannot see `test()` names) and
      four sites in `just/native.just`, including a `--test cpp_parameters` that
      would have failed outright.

### W5 — the runtime-compile test is a labelled exception, not a defect (LANDED)

**This item was based on a wrong reading of mine, and the correction is the
finding.** `cmake_platform_matrix.rs` does not build anything: it runs
`cmake -S -B` (configure only, seconds) and asserts the configure **FAILS** with
`NANO_ROS_BOARD` named in the FATAL_ERROR.

That cannot become a build-stage fixture. A fixture whose configure fails fails
the BUILD — the artifact the build stage would produce is precisely the artifact
that must not exist.

The same holds for every other test that reaches a toolchain at run time here:
`*_misuse`, `negative_diagnostic_registry`, `diagnostic_verbatim`,
`zpico_drift_gate`. All are NEGATIVE tests asserting a diagnostic that only
exists on the failure path.

- [x] Labelled the exception and its general shape in `cmake_platform_matrix.rs`.
      The rule's target is a test that BUILDS ITS OWN FIXTURE and then uses it;
      these build nothing they keep.

**Also corrected:** `zpico_build_matrix.rs` looked like a second violation and is
not one — it runs `cargo tree`, a metadata query.

## Acceptance (phase)

- No live nextest `test()` filter matches zero tests (W1).
- One repo-root helper (W2).
- `check-no-vacuous-tests` and `check-nextest-binary-filters` green in `just check`
  and still self-testing (W0, holds).
- Each remaining `tests/` file is either a matrix consumer, a labelled one-off, or
  a gate — no file whose tests only print, and none subsumed by a stronger cell.

## Non-goals

- Chasing the test-file COUNT as a target. Phase 329 restated its own `≤120` to
  the measured `151` once the disposition showed most candidates were genuine
  one-offs. Deleting a real one-off to make a number move is the failure this
  campaign is correcting, not repeating.
- Static checking of `test()` filter names. They are rstest-generated case names
  (`Platform__Nuttx`) appearing literally nowhere in the sources; deriving them
  needs a compiled `cargo nextest list`. W1 fixes the instance by hand and the
  gate stays scoped to `binary()`, which is the fatal half.
