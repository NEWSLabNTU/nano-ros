# Phase 475 — which test targets a lane can run: a census, not an inference

**Status (2026-10-02). W0, W1 LANDED; W2 (the reds) LANDED; W6 DECIDED (the
package is in); W3–W5 open.** A design study with its measurement done first. The question is
issue 0922's, asked for every `nros-tests` target at once: *which of them reach
a verdict in which lane?* The answer it reaches is that **only the lane's own
environment can say, and asking it costs 24 seconds** — so the classifier is a
census run there, and nothing in this phase infers preconditions from source.

Related: issue 0922 (excluded by crate where the property is per target), issue
1226 (a gate that works is not a gate that runs), issue 0584 (skips countable,
then checkable — its open residue is this phase from the other side), issue
1452 (a reach wider than the rule), issue 1610 (the red that started this),
RFC-0061 (tiers). Admission into a lane stays governed by
`check-lane-contracts`' rule: *a lane may only resolve what its own job builds.*

## The problem, as it presented

`loc_budgets` sat red on `main` for three days (issue 1610). It needs nothing —
it counts source lines — but `test-unit` runs `--workspace --exclude
nros-tests`, so it reached only the fixture lanes, and the fixture lane on
`main` has produced no green run in its last 200. Nothing between the commit
that broke it and that commit's merge asked the question.

That is not one test's misfortune. `nros-tests` holds 182 test targets and the
only ones a merge-gating lane runs are the three `test-lane-contracts` names —
four, with #1514. Everything else is decided **per crate**, which is issue
0922's defect stated in its own words.

## W0 — why the obvious classifier is wrong, measured

### The existing static answer

`check-lane-contracts` has `resolvers_used(target)`, a text scan for calls to
the RUNTIME fixture resolvers. A target calling none reads as fixture-free.
Applied to all 182 it reports **83** — and that set contains `rtos_e2e`,
`fvp_smoke`, `qemu_patched_binary` and `ros_editions_e2e`, which need QEMU, an
FVP and docker. It is not wrong about fixtures. It is wrong because fixtures
are one precondition among several, and it was only ever asked about one.

### Preconditions are written four ways

Measured over all 182 targets (a target may use several):

| encoding | targets | visible to a name scan? |
| --- | --- | --- |
| touches a fixture resolver or `require_*_in_lane` | 127 | yes |
| calls a library probe (`is_qemu_available`, `require_zenohd`, … — 62 of them) | 118 | yes, by name |
| reaches a helper module that probes (`ros_env`, `qemu`, `ros2`, `zephyr`, …) | 33 | only with a call graph |
| an inline condition guarding `skip!` (`if !Path::new(fvp).exists()`) | 29 | **no** |
| a test-LOCAL probe (`fn require_patched_qemu` inside the test file) | 22 | only if named like a probe |
| none of the above | 24 | — |

A static classifier has to get all five rows right, and the third and fourth
are not decidable from names. Each fix to the scanner moves the error rather
than removing it — issue 1452's direction (cargo's own `OUT_DIR` read as a
path, 55 times) and issue 0196's (an inline condition invisible) at once.

### The census

The gate lane runs in `ghcr.io/newslabntu/nano-ros-ci:humble`. That package is
private and refuses an anonymous pull, so the image was **built locally from
`ci/docker/ci-base/Dockerfile`** — the same file `images.yml` builds it from —
and every `nros-tests` target was run inside it with the gate job's CLI build
reproduced and **no fixtures staged**.

What the image provides, probed inside it rather than read off the Dockerfile:

| present | absent |
| --- | --- |
| `ros2` (humble), `cmake`, `ninja`, `clang`, `arm-none-eabi-gcc`, `riscv64-unknown-elf-gcc`, `socat`, `cargo-nextest`, `just` | `qemu-system-*`, `docker`, `zenohd` / `rmw_zenohd`, `west`, `MicroXRCEAgent`, `espflash` |

**878 test cases across 167 targets ran in 24 seconds.** That number is the
design's load-bearing fact: an unmet precondition ends a test in milliseconds,
so measuring the whole crate in the lane's own environment is cheaper than
most single gates on the fast line.

| outcome | targets | meaning |
| --- | --- | --- |
| PASS | **45** | reached a verdict with nothing staged |
| FAIL | 5 | reached a red verdict, or hid a precondition as a failure |
| FIXTURE | 35 | needs a staged fixture |
| SKIP | 82 | a precondition this image lacks |
| TIMEOUT | 0 | — |
| not run | 16 | 13 behind `required-features`, 3 others |

**Census artifacts, removed before the table above.** The first run reported 11
FAILs. Six were `git ls-files failed: not a git repository` — a linked
worktree's `.git` is a FILE naming the main checkout's gitdir, which the
container could not see. The gate checks out a real repository, so those six
were about the census, not the code; the gitdir is now mounted read-only at its
own absolute path, and `lane-census.sh` refuses to run if `git` is unusable.

### What the five FAILs were — each re-run on the host

| target | verdict | |
| --- | --- | --- |
| `loc_budgets` | **real red on `main`** | issue 1610; fixed in #1514 |
| `example_portability::copies_within_a_group_are_identical` | **real red on `main`** | `rust/listener` copies diverge: mps2-an385-freertos and native differ from esp32-c3-baremetal |
| `no_local_axis_tables` | **real red on `main`** | `tests/qos_event_interop.rs:72` defines `QOS_EVENT_CELLS`, a second matrix spelling RFC-0051 forbids |
| `params_per_node_interop::cases_bound_to_interop_cells` | **real red on `main`** | the test's `#[case]`s cover `(0,0,0,70)`; `interop::CELLS` declares `(0,0,1,70)` as well |
| `native_orchestration_misuse::launch_arm_resolves_the_bringup` | precondition reported as FAIL | needs `nros-launch-resolve`, which the real gate job builds; it also runs `cargo build` at test time, which CLAUDE.md forbids |

**Four reds on `main`, all fixture-free, all invisible to every merge-gating
lane.** `loc_budgets` was the one somebody happened to run.

### What the SKIPs say about the image

| missing | targets blocked |
| --- | --- |
| **`rmw_zenohd`** | **32** |
| a staged fixture, reported as a skip | 14 |
| a ROS package or RMW | 8 |
| QEMU (one of them the patched build) | 6 |
| `nros-launch-resolve` — **which the real gate job builds** | 3 |
| a docker image (`ros_editions` jazzy) | 3 |
| FreeRTOS sources | 3 |
| NuttX, an ARM FVP, out-of-lane | 2 each |
| the XRCE agent, ThreadX, Zephyr/west | 1 each |
| other (aggregate cells that skipped every sub-cell) | 4 |

The `nros-launch-resolve` row is the lower bound showing: those three, plus
`native_orchestration_misuse`, very likely reach a verdict in the real lane,
which builds the resolver before `test-lane-contracts` runs.

One apt package, `ros-humble-rmw-zenoh-cpp`, looked like the difference between
the gate image and **32** more targets reaching a verdict. **It was not, and the
census is what showed it** (W6): a skip names the FIRST unmet precondition, not
the last, so a count of "skipped for X" is an upper bound on what X alone buys.

The 14 *fixture-as-skip* targets are worth their own look: issue 0584's
`check-skip-budget` asserts that a missing fixture is a hard failure, never a
skip, and these report one as a skip. Either they are the three laundering
sites 0584 names firing, or there are more of them.

## The design

**The classifier is the census.** A target is admissible to a lane iff, run in
that lane's environment, it reaches a verdict. Nothing else answers that
question exactly, and the measurement above shows nothing else is needed: it is
fast enough to run, and its result is a derived artifact rather than a list
somebody keeps.

Four parts, each with a different job:

1. **Census** (W1, landed) — `scripts/test/lane-census.sh <image>` runs every
   target in the image and `lane-census-classify.py` reduces the junit to one
   outcome per target. Reproducible by anyone with the image; no inference.
2. **Admission** (W3) — a lane's target list is GENERATED from its census:
   every target whose outcome is a verdict. Committed, because a merge-gating
   run needs a fixed list, and regenerated rather than edited.
3. **Verification** (W4) — the committed list goes stale the day a target
   gains a precondition. So a lane that admits by census asserts one more
   property, in issue 0584's style (a property, never a count): **an admitted
   target that skips fails the lane**, naming the target and what it skipped
   on. That bounds the census's error to one direction — a false admit is
   caught on the next run; only a false *exclude* survives, and the periodic
   census finds those.
4. **Periodic census** (W5) — on schedule, in the published image, reporting
   the diff: newly admissible, newly red, and the missing-capability table.

Static scanning keeps the job it is good at. `resolvers_used()` stays in
`check-lane-contracts` as the fast pre-check that no ADMITTED target calls a
runtime fixture resolver. It is a guard on admission, not the classifier.

### Alternatives considered, and why not

* **Infer preconditions statically.** Needs a call graph over `nros-tests`'
  helpers plus reading inline conditions, and still errs both ways. Measured:
  the existing static classifier was wrong about four named targets in one
  pass.
* **Record needs at run time by instrumenting the probes.** Have each probe and
  fixture resolver log its own name, run every target once on a fully
  provisioned host, and admit when the logged needs fit what the lane provides.
  Exact for library probes, but blind to the 29 inline conditions and to the 22
  local probes that are not shaped like one. PROVIDES still has to be measured
  in the lane — so this needs the census's environment anyway, plus a
  registry and a migration. Kept as a refinement if the census ever stops being
  cheap; at 24 seconds it is not needed.
* **Keep excluding by crate.** The status quo, and the defect.

## Work items

### W0 — measure (LANDED)

Above. Reproduce with W1.

### W1 — the census tool (LANDED)

`scripts/test/lane-census.sh` + `scripts/test/lane-census-classify.py`. Mounts
the checkout read-only and copies it inside the container, so nothing
root-owned reaches the host; mounts a linked worktree's gitdir at its own path;
refuses to report if `git`, the CLI build or the `nros-tests` build fails,
because a census of a broken build is a census of nothing.

### W2 — the four reds the census found (LANDED)

`loc_budgets` was fixed by #1514; the other three in the PR that landed W6.
Each was a DIFFERENT side going stale, so each fix went a different way —
recorded in issue 1620, which stays open for what remains
(`native_orchestration_misuse`, the 14 fixture-as-skip targets, and two tests
that compile at run time and contend under a parallel census).

`example_portability` turned out to hold **three** divergences, not one: the
first read of the failure stopped at its first entry. One of them,
`rust/service-client`, had been red since 2026-09-06.

### W3 — admission from the census

`test-lane-contracts` holds a hand-written list of four. Replace it with a list
generated from the gate image's census. Its name is already narrower than its
rule (#1514 said so); rename it for what it runs. W2 must land first: admitting
the census's verdict set today would admit three reds and turn the lane red for
reasons nobody on the PR caused.

### W4 — an admitted target that skips fails the lane

A third property for `check-skip-budget`, beside 0584's two. Acceptance: a
target in the admitted list that skips — any class — makes the lane red and
says which precondition it met, with a negative control that plants exactly
that.

### W5 — the periodic census

On `schedule`, in the published `nano-ros-ci` image, uploading the per-target
table and the missing-capability ranking, and failing only on a TIMEOUT (a test
that hangs instead of skipping is always a defect).

### W6 — the image lever (a decision, not a task)

**DECIDED 2026-10-02 — added**, and re-measured with it in.

The census with the router present: **0** targets still skip on `zenohd`, and
**0** of the 32 became PASS. Four moved SKIP → FIXTURE — they now get past the
router and stop at a fixture, which this census never stages — and the rest
stop at the next precondition they meet. For the gate lane, which stages no
fixtures, the package buys nothing yet.

It was added anyway, for the lane that DOES stage fixtures: `host-tests` runs
in the same `nano-ros-ci` image, so every zenoh interop target there was
skipping on the router before reaching the fixtures it had built. That benefit
is the expected one, not a measured one — `host-tests` has no green run to
compare against (issue 1500).

The lesson is about the census rather than the package. **A skip reports the
first missing precondition, so "N targets skip for X" is an upper bound on
what X alone buys.** W5's missing-capability ranking must say so, or it will
send the next image change after a number that is not real.

The package also changed the image's frozen `ENV`: sourcing ROS now puts
`/opt/ros/humble/opt/zenoh_cpp_vendor/lib` FIRST on `LD_LIBRARY_PATH`. The
snapshot was re-captured, and `rmw_zenohd` was confirmed to bind that
`libzenohc` and start — issue 0774's distinction between a router that
resolves and one that runs.

## What this phase deliberately does not do

* **Admit anything yet.** W3 waits for W2, for the reason above.
* **Count skips per class as a budget.** 0584 refused that, correctly — counts
  drift with every host. The verification in W4 is a property.
* **Fix the 14 fixture-as-skip targets.** Measured, named in issue 1620, and
  0584's territory.
* **Claim the admissible set is 45.** It is at least 45. The census provisions
  less than the real gate job (no compile-tier sources, no compile-check
  fixtures, no bindings, no launch resolver), so a target needing one of those
  appears as a precondition here and may well pass there.
