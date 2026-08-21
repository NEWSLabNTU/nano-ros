# Phase 373 — Test-suite cleanup: delete what cannot fail, consolidate what was copied

**Status (2026-08-21): W0 LANDED (`a660be83f`), W1–W5 OPEN.** W0 removed 23 tests
that could not fail and gated both shapes that admitted them. The remaining items
come from the survey W0's fallout prompted; **W1 is a live defect, not a cleanup**,
and should land first.

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

### W1 — the `zephyr-qos-port` resource group is off (DEFECT, do first)

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

- [ ] Move `qos_zephyr_ros2_interop_e2e` into `matrix-consumers-serial`; retire
      `zephyr-qos-port` and its now-single-use override.
- [ ] Re-check the sibling `native-qos-discovery` and the phase-296 W5.5 group for
      the same shape — a per-cell `test()` naming a folded cell.
- [ ] Verify by RUNNING nextest, not by `just check` (which runs none): confirm
      the two land in one group and cannot overlap.

**Acceptance:** no live `test()` filter matches zero tests, and the two port
sharers are mutually exclusive.

### W2 — one repo-root helper, not 23

`nros_tests::project_root()` already exists. Beside it the tests carry **23** more
declarations under three names (`workspace_root` ×11, `repo_root` ×6,
`project_root` ×6):

- 17 hand reimplementations (`ancestors().nth(3)`, chained `.parent().unwrap()`)
- 4 trivial aliases (`fn workspace_root() -> PathBuf { nros_tests::project_root() }`)
- 2 `canonicalize()` variants — **not equivalent**: they resolve symlinks, so they
  disagree with the other 21 under a symlinked checkout

- [ ] Delete all 23; call `nros_tests::project_root()` directly.
- [ ] Decide the symlink question explicitly rather than by accident — if any test
      NEEDS a canonical path, that belongs in the lib helper for everyone.
- [ ] Gate it, or the count grows back: a `fn` in `tests/` that only re-exports a
      lib helper is the same "second spelling" CLAUDE.md's class rule forbids.

**Acceptance:** one definition; `git grep -c 'fn workspace_root\|fn repo_root'`
over `tests/` returns 0.

### W3 — the matrix consumers re-grew a shared prelude

Each consolidated consumer copied the same helpers: `lang_str` (8 files),
`run_cell` (7), `spawn_listener` (7), `exec_for` (6), `fixture_dir`/`fixture_src`
(6). This is phase-329's consolidation paying its cost in the other currency —
fewer files, but the boilerplate each one needs is now duplicated per consumer.

- [ ] A shared support module (`nros_tests::cells`) holding the cell-running
      vocabulary; consumers import it.
- [ ] Do it AFTER W2 — W2 is mechanical and will surface how much of this is
      genuinely shared versus superficially similarly-named.

**Acceptance:** each helper has one definition, or a documented reason why a
consumer's variant differs.

### W4 — fold the single-test micro-files by family

19 files hold one test in under 80 lines. The families are obvious:
`cli_bringup_{esp_idf,zephyr,nuttx}`, `{nuttx,threadx_linux}_entry_build`,
`ros_editions_{smoke,bridge}`, `{c,cpp}_parameters`.

- [ ] Fold each family into one parametrized file, applying phase-329's rule:
      fold a per-cell duplicate, KEEP + LABEL a genuine one-off.
- [ ] Coverage-proven per deletion, exactly as phase-329 W4 required: no file
      removed until the surviving test demonstrably runs its cases.

**Acceptance:** every fold names the cases it absorbed; no case lost.

### W5 — the last runtime-compile test

`cmake_platform_matrix.rs` configures and BUILDS a synthetic consumer project at
test time, which CLAUDE.md forbids ("No compilation inside tests"). Phase 329 W5
moved this class to the build stage and this one did not follow.

- [ ] Either express it as a build-step fixture asserting the artifact, or record
      why the cmake-contract shape cannot be one (a synthetic consumer project is
      not obviously a `fixtures.toml` row) and label it as a deliberate exception.

**Correction to an earlier reading of this item:** `zpico_build_matrix.rs` looks
like a second violation and is NOT one — it runs `cargo tree`, which is a metadata
query, not a compile.

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
