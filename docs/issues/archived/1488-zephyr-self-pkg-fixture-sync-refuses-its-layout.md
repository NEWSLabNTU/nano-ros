---
id: 1488
title: "`zephyr_self_pkg`'s bringup dirs have no `package.xml`, so the `nros
  sync` the west fixture builder runs there is refused outright — and the
  configure that needs its SystemModel then fails"
status: resolved
type: bug
area: testing, build, cli
severity: high
resolved: 2026-09-28
related: [1501, 1497, 1458, 1312, 0510, 0533, phase-330, phase-445]
---

## Symptom

`just build-test-fixtures lane=all` fails in the zephyr module:

```
   nros sync failed in alpha_pkg (configure may fail)
...
CMake Error at zephyr/cmake/nros_system_generate.cmake:316 (message):
  nros codegen-system failed (rc=1):
  Error: codegen-system:
  .../fixtures/zephyr_self_pkg/self/alpha_pkg/system.toml declares system
  semantics but no SystemModel was found. It is a BUILD ARTIFACT
  (phase-330 W4), so generate it rather than committing one:
      nros sync      # writes <ws>/build/nros/models/<bringup>/
   MISSING nros-system/system_config.h for zephyr_self_pkg_rust
```

Two targets, `zephyr_self_pkg_rust` and `zephyr_self_pkg_sibling`. The module
builds first in `lane=all`, so this starves every module behind it.

## Cause

Two independent defects, and this file originally attributed both to one.

**(a) No producer for the model.** `nros sync`'s package scan has exactly two
shapes — a colcon workspace (`src/<pkg>/package.xml`) or a single-package dir
(`package.xml` at the root) — and rejects a directory with neither:

```
Error: sync: no `src/<pkg>/package.xml` and no `package.xml` at root under
  .../zephyr_self_pkg/self/alpha_pkg — expected colcon-style workspace or single-pkg dir
```

Both bringup dirs held `Cargo.toml`, `CMakeLists.txt`, `prj.conf`, `src/` and
`system.toml`, and **no `package.xml`**. This half is issue **1501**, resolved
2026-09-25.

**(b) The failure was swallowed.** `west-fixtures.sh` ran sync per bringup and
continued regardless, so the lane's verdict came from the configure — five lines
about a missing build artifact — rather than from the refusal that explains it.
That is the 0510 masking shape the block's own comment already cited. Issue 1501
made the step print sync's output (`cff7110a3`); it left the step **non-fatal**,
which is what this issue closes.

### What actually regressed it — the original narrative was wrong

This file said "The fixture arrived with phase-445 W5 (2026-09-11)". It did not.
`git log --diff-filter=A --name-only -- 'packages/testing/nros-tests/fixtures/zephyr_self_pkg/*'`:

| Commit | Date | Added |
| --- | --- | --- |
| `f5efde612` | 2026-06-13 | the trees (`CMakeLists.txt`, `Cargo.toml`, `prj.conf`, `src/lib.rs`, `sibling/caller/*`) |
| `a12e2c3e4` | 2026-08-13 | the `[[compile_check_fixture]]` rows (phase-350 W2) |
| `2bb20c231` | 2026-09-11 | **only** the two `system.toml` files (phase-445 W5) |
| `1879c31c4` | 2026-09-25 | the two `package.xml` files (issue 1501) |

So W5's contribution was putting a `system.toml` into dirs that had no
`package.xml`. That is precisely what makes the builder run `nros sync` there —
it iterates the immediate subdirs **holding a `system.toml`** — and what makes
the configure demand a SystemModel. `2bb20c231` is the regressing commit; the
fixture is three months older than the bug.

(Also: `379f7c8d2` is `2026-09-18 03:13 +0800` = 2026-09-17 19:13 UTC, so the
"2026-09-17" above is right only in UTC.)

## Which of the two routes the code supports

The `package.xml` route is the one the code already resolves, and it is the one
1501 took. `_nros_system_detect_self_pkg` accepts a dir as a self-pkg bringup on
`system.toml` **plus** (`Cargo.toml` or `CMakeLists.txt`) — it never looks for a
`package.xml` — so the manifest is invisible to the shim and cannot perturb the
bringup resolution it already performs. `nros sync` is the only producer of the
artifact the declaration makes mandatory, and a root `package.xml` is one of its
two accepted shapes. MEASURED below: sync accepts it, and the fixture's own
build then consumes the model rather than choking on the manifest.

The alternative ("stop running sync where it cannot work") was not available:
the configure has no other road to a model, so it would have meant unmaking
`2bb20c231` for these two leaves.

## Fix

1. (1501, 2026-09-25) `package.xml` in both bringup dirs, plus gate
   `check-self-pkg-package-xml`.
2. (this issue) **the sync failure is now the ROW's failure.** It goes through
   the `failed` tally the loop already keeps, so the existing exit check at the
   foot of `west-fixtures.sh` makes it fatal — explicitly, with or without
   `set -e` (only `set -u` is in force there). The status is still captured at
   the command, never at a later assignment (issue 1249).

   The west build is **skipped** rather than run and discarded. Every directory
   this loop syncs is a bringup whose model that same row's configure reads —
   measured over the four west fixture source dirs the five rows share:
   `board_import_fvp` has none, the other three have exactly one each, and none
   has a bringup some other row owns — so a configure past a failed sync is a
   guaranteed refusal for a reason already printed.

   An absent CLI is folded into the same verdict. The old code wrapped the whole
   block in `if [ -x "$_wf_cli" ]`, so a row with a bringup and no `nros` skipped
   sync silently and handed the configure the same missing model.

## Measured

`nros sync` in both bringup dirs, on this checkout (the exact failing command):

```
=== self/alpha_pkg ===     rc=0, wrote build/nros/models/alpha_pkg/system_model.yaml
=== sibling/alpha_pkg ===  rc=0, wrote build/nros/models/alpha_pkg/system_model.yaml
```

The fatal path, both directions — the real script run with a stub `nros` and a
recording stub `west` against a fake `ZEPHYR_BASE`, so the skip is keyed on the
sync failure and nothing else:

```
stub nros exits 1 -> script exit 1; the three rows with a bringup each report the
                    refusal and print SKIPPED; west invoked ONCE, for
                    `west_board_import`, the row with no bringup
stub nros exits 0 -> all five rows reach west; 5 invocations recorded
```

**The configure, for real.** `west build --cmake-only` with the exact argv the
builder emits for each row (captured from the stubbed run above). Both rows
produce the pair `packages/testing/nros-tests/tests/zephyr_self_pkg.rs`
(`assert_bake`) asserts:

```
build/west-fixtures/zephyr_self_pkg_rust/nros-system/{system_config.h,system_config.cmake}
build/west-fixtures/zephyr_self_pkg_sibling/nros-system/{system_config.h,system_config.cmake}
#define NROS_SYSTEM_NAME "alpha_pkg"
#define NROS_SYSTEM_DOMAIN_ID 0u
```

No Zephyr workspace is provisioned in this worktree, so this ran against a
private west topdir built by `cp -al` from the main checkout's Zephyr 3.7 tree,
with an UNBOUND manifest project (issue 1258) and the nano-ros module named per
build by `-DZEPHYR_EXTRA_MODULES=<this worktree>`. Pointing `ZEPHYR_BASE` at the
main checkout's own workspace was rejected as a measurement: its manifest project
`nano-ros` is a symlink to that checkout, so the configure would have resolved
the module from a different tree than the one under test (issue 1280's shape).
Symlinking `zephyr` into a topdir does not work — Zephyr resolves the west topdir
from `ZEPHYR_BASE`'s REAL path.

## Had these targets ever been built successfully? YES — they broke

Acceptance item 4, answered from CI rather than inferred. **Green through
2026-09-10, red 2026-09-22…09-25, green again since 2026-09-26.** It broke; it
was not "never exercised", and the original file's "both are consistent with the
evidence" is now settled.

Directly observed. `run-matrix.yml` run **36387679482**, schedule,
2026-09-28T06:40Z — `gh run view 36387679482 --log` prints `== zephyr == OK`, and
the run's `post-submit-junit` artifact
(`tmp/build-test-fixtures-*/zephyr.log:3358-3360`):

```
   ok .../build/west-fixtures/zephyr_self_pkg_sibling (nros-system/system_config.h)
west fixtures: 5/5 ok (0 reused, 5 built).
Zephyr test fixtures built successfully.
```

`grep -c "nros sync FAILED"` on that log is **0**, and `0 reused` means freshly
built rather than a carried stamp. Runs **36300163622** (09-27) and
**36223768394** (09-26) are identical.

The last red, **36103083615**, 2026-09-25T07:07Z, is this issue's symptom
verbatim:

```
== zephyr == FAILED (rc=1)
  declares system semantics but no SystemModel was found.  It is a BUILD
   MISSING nros-system/system_config.h for zephyr_self_pkg_sibling
west fixtures: 3/5 ok (0 reused, 3 built).
west-fixtures: 2 of 5 fixture(s) FAILED to build.
```

1501's commits landed 2026-09-25 20:57 +0800 (≈12:57 UTC), between that red and
the 09-26 green — so the `package.xml` is what flipped it, confirmed by run
boundary rather than by reasoning. The `4 of 5 FAILED` on 09-22/09-23
(**35694989528**, **35826999550**) is issue 1458's Kconfig kill; `7338bb87c`
(09-23) dropped it to `2 of 5`, leaving exactly these two rows on the SystemModel
refusal.

Before the break the evidence is an INFERENCE, not a line in a log — those runs'
failure tail shows the tier-priority output, which is past the `west fixtures:
N/M ok` summary, so that summary is not in them. The inference is the lane's
fail-closed structure: runs **34194794286** (09-08) and **34445356808** (09-10)
print `== zephyr == FAILED (rc=1)` and their ONLY failure marker is
`tier-priority-plan-image: FAILED`. `just/zephyr-ci.just`'s `build-fixtures` sets
`set -e`, then runs `west-fixtures.sh` — no `|| true`, issue 0700 — then the
success echo, then `check-tier-priority-plan-image.py`; and `west-fixtures.sh`
exits 1 when `failed != 0`. Reaching the tier-priority script therefore requires
every west row to have produced its `output`. Checked at the tree of the time,
not just today: at `b5097a460` (main tip 2026-09-09) that ordering is
`zephyr-ci.just` lines 25 / 399 / 400 / 405, the `exit 1` is present in
`west-fixtures.sh`, and `examples/fixtures.toml` carries the same
`output = "nros-system/system_config.h"` gate for both rows.

Not established, and it does not change the verdict: no CI covers before
2026-09-01 (`run-matrix.yml`'s oldest run is 33477831186, 2026-09-01T06:29Z; the
nightly ran no zephyr stage then; `build-wide.yml` has been dormant since
2026-09-03). Logs for 09-14/-15/-16 are gone (cancelled runs; `log not found`),
and per-stage `zephyr.log` artifacts exist only for 09-17 onward (14-day
retention). An older green is recorded in
`docs/roadmap/archived/phase-350-west-fixtures-join-the-manifest.md` (2026-08-13,
"3/4 before and 3/4 after … same fixture failing — `west_bringup_zephyr`", plus
`zephyr_self_pkg_rust | 3 s, 3.0 MB`), which is a doc rather than a log.

## A trap for the next reader

**"Built successfully" for these two rows has never meant "west exited 0", and
still does not.** Even in the green 09-28 log, and identically in the local
configure above:

```
CMake Error at .../zephyr/cmake/modules/extensions.cmake:428 (add_library):
  No SOURCES given to target: app
CMake Generate step failed.
FATAL ERROR: command exited with status 1: /usr/bin/cmake ... -B.../zephyr_self_pkg_rust ...
   ok .../west-fixtures/zephyr_self_pkg_rust (nros-system/system_config.h)
```

By design: the app is a Rust self-pkg with no C sources, and the row's contract
is the configure-time BAKE, not a link — `west-fixtures.sh` ("the stamp gate is
`output` EXISTS, for both builders — not west's exit code"), `examples/fixtures.toml`
and `zephyr_self_pkg.rs`'s own doc comment all say so. Anyone grepping
`FATAL ERROR` will read a green lane as red.

## Observed alongside, NOT fixed here

Reaching the configure surfaced a second absorbing state, in a different tool,
which deserves its own issue (not filed from this session — an id must be
reserved with `just issue-new`, which pushes a ref):

**`nros sync` exits 0 while writing a model the bake then rejects, and the
bake's remedy re-runs the tool that wrote it.** With `nros-launch-resolve`
absent, sync reported `resolved system.launch.xml → …/system_model.yaml` and
`done.` at rc=0, having recorded the resolver pin as `unknown`. The configure
then refused:

```
Error: codegen-system: SystemModel `…/system_model.yaml` is stale:
  resolver pin changed (model `unknown` != ours `67dc769105e5`)
    Run `nros sync` to re-resolve it.
```

Re-running `nros sync` reproduces the same model. The actual remedy is
`just setup-launch-resolve` first (then `just setup-cli`, since the `play_launch`
pin is a CLI source-stamp input — issue 1018). Same shape as this issue one tool
over: a step that cannot do its job succeeds, and the refusal lands somewhere
else naming the wrong fix.

## Not verified here

* **The full `west-fixtures.sh` lane was not run.** It would also build the two
  `west-build` rows (full `native_sim` images), which this question does not need
  and the disk could not afford. What was run is the per-row argv for the two
  rows in question, and the whole script under stubs for the failure path.
* **No runtime.** `zephyr_self_pkg.rs` inspects the bake and launches nothing, so
  there is nothing further to run for these rows.
* The two bringup dirs' `nros sync` was measured on a host with **no ROS**; sync
  reports `no producer for alpha_pkg::selfpkg` (a deploy-bound metadata probe
  that cannot build for a `no_std` target here) and still exits 0 and writes the
  model. That warning is present in the green CI logs too.
