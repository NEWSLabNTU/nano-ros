# Phase 368 — What a reader actually runs, per platform and per language

**Status (2026-08-19).** IN PROGRESS — W1, W2, W3, W5, W6, W7 landed; W4 in flight.

Implements the fix for [issue 0694](../issues/0694-platform-starters-omit-nros-sync.md).

## The finding

The book documents the current workflow **only where a Linux reader lands**.
`nros sync` — the step without which no Rust leaf can even be parsed by cargo —
appears on 13 of 107 pages, and none of the 12 platform starters is one of them.
Those 12 pages carry 26 build commands between them, and every Rust one of those
26 fails on a fresh clone with an error whose four frames never name the missing
step.

The gap is not that the pages are old. `user-guide/workflow.md` was touched
yesterday and still presents `nros metadata` / `nros plan` / `nros check` as the
multi-component flow, with no `nros sync` anywhere. Recency of edit is not
currency of content.

## The organizing fact

The tree already answers "what do I run?" precisely, and the answer is keyed on
the **builder**, which follows from the language — not on the platform. From the
413 rows of `examples/fixtures.toml`:

| builder | languages | platforms it appears on |
|---|---|---|
| `cargo` | rust, mixed | every one (linux, freertos, nuttx, nuttx-riscv, threadx-linux, threadx-riscv64, zephyr, esp32, both baremetals) |
| `cmake` | c, cpp, mixed, rust | linux, freertos, nuttx, nuttx-riscv, threadx-linux, threadx-riscv64, zephyr |
| `west` | c, cpp, rust | zephyr, zephyr-cortex-m |

And the sync requirement partitions on exactly that seam: a tracked leaf
`.cargo/config.toml` — the file whose `include` is the trap — exists under
`rust/` leaves and nowhere else. So:

- **Rust (cargo, including cargo under west)** — `nros sync` before the first
  build, on every platform.
- **C / C++ (cmake)** — no sync. Codegen runs inside cmake via
  `nros_find_interfaces()`; the cargo builds cmake drives resolve against the
  repo-root `.cargo/config.toml`, which carries no `include`.

Stating that rule once, and routing to it, is the phase. Pasting `nros sync`
into 12 files is not — it would leave the C/C++ reader running a step they do
not need and teach nobody why.

## Work items

**W1 — the missing step, stated as the rule. LANDED.** Three starter pages
hand the reader a bare `cd <leaf> && cargo build`, and those three are the ones
that break: `freertos.md`, `threadx.md`, `bare-metal.md`. Each now runs
`nros sync` first and says what the failure looks like if you skip it. The
remaining nine reach their builds through `just` recipes, `west`, `make` or
`idf.py`, all of which sync (or drive codegen) themselves — they were not
broken, and pasting the step into them would teach a requirement that is not
theirs.

Verified rather than argued: moving the generated `nros-patch.toml` aside
reproduces the five-frame parse failure in
`examples/qemu-arm-baremetal/rust/talker`; `nros sync` there regenerates the
file byte-identically and the build proceeds past it. No C/C++ block gained a
step.

The finding that made the scope decision: **every `just` build path already
syncs** (`fixtures-build.sh`'s pre-pass, issue 0649) — so the one path in the
repo that skips it is the copy-pasteable leaf build the book teaches. CI
exercises the recipes; the book teaches the leaf.

**W2 — the platform × language workflow page. LANDED.**
`book/src/user-guide/workflow-by-platform.md`, first entry under User Guide.
The builder grid is generated from the 413 coordinate rows of
`examples/fixtures.toml`, so it states what CI executes rather than what the
prose remembers. All nine `nros setup <board>` names in its per-platform table
were validated against the CLI with `--dry-run`; the `just` module names come
from `just --list`.

Two claims were written, checked, and corrected before landing rather than
after: `[build] target` is set by the Cortex-M leaves only, not universally
(zephyr, threadx-linux and nuttx Rust leaves do not set it), and the Arm FVP
page provisions with `nros setup zephyr` plus a license-gated binary, not the
`fvp-aemv8r-smp` index row. Both would have read as authoritative.

**Regenerating W2's builder grid** (so it can be re-derived rather than
re-remembered — it will drift as cells are added):

```bash
python3 scripts/build/fixtures-manifest.py coords | python3 -c "
import sys, collections
cells=collections.defaultdict(set)
for ln in sys.stdin:
    f=ln.rstrip(chr(10)).split(chr(31))
    if len(f)>=8: cells[(f[1],f[2])].add(f[7])
plats=sorted({k[0] for k in cells})
for p in plats:
    print(p, [','.join(sorted(cells.get((p,l),['-']))) for l in ('rust','c','cpp','mixed')])
"
```

**W3 — currency of `user-guide/workflow.md`. LANDED.** Its step 5 showed
`nros metadata` / `nros plan` / `nros check` as the multi-component build path.
Those commands are real, but they are the INSPECTION path — they produce and
validate an `nros-plan.json`. The build path, per the header of
`workspace-fixtures-build.sh`, is `nros sync` → `nros codegen-system --bringup
<b> --out <o>` → `cargo build -p <entry>` (or the cmake target). The page now
says that, notes which single-example builds need sync, and describes
SystemModels as build artifacts an entry names by INPUT rather than by path.

Recency of edit is not currency of content: this page was touched the day
before the audit and was wrong about the whole multi-node flow.

One correction found in review: the draft recommended `nros ws model-dims` for
inspecting a resolved model, which CLAUDE.md also suggests. That subcommand is
`hide = true` — its own doc-comment says it exists so `check-model-dims.sh` can
ask. Pointing users at a hidden gate seam is not documenting a workflow; the
page says `nros model-path` instead, which is public and prints the resolved
path.

**W4 — probe coverage. IN FLIGHT.** `probe=` covered 3 blocks on 2 pages, and
both were pages that already had the step right; the pages that were wrong were
exactly the unprobed ones.

`first-node-c.md`'s build block is now `probe=40`, which makes the clean-room
run assert the claim W2's page makes — that a C leaf needs no `nros sync`,
because its bindings are generated inside CMake and it carries no
`.cargo/config.toml`. A prose claim about a missing step is exactly the kind
that should be executed rather than believed.

The book's C chapter is written for a reader at the repo root; the probe is ONE
shell sitting in `examples/native/rust/talker` when step 40 begins. That is what
`--subst` is for, and the substitution resolves through `git rev-parse
--show-toplevel` rather than a literal `$HOME/nano-ros`, so it does not assume
where the clone landed — the same move `verify-first-node.sh` already makes.
Extraction verified with `PROBE_EXTRACT_ONLY` (4 steps, correct cd).

Remaining before this can be called landed: the container run itself.

Out of scope, and stated rather than quietly skipped: starters needing hardware,
a vendor SDK, or a QEMU image the probe container does not provision. A
cross-compiled Rust starter would need `nros setup qemu-arm-freertos` inside the
container — a large fetch for one assertion. For those pages W2's table is the
guarantee, and this phase does not pretend otherwise.

**W5 — in-checkout vs copied-out, said once.** Found while verifying W1: the
starter pages' `cd <leaf> && cargo build` writes `examples/**/target/`, which
`check-example-leaf-target-dirs` rejects (269 MB from one Cortex-M leaf, and a
red `check-fast`). The command is not wrong — it is right for a copied-out
example and wrong inside the checkout, and no page says which the reader is in.
W2's page now draws the distinction. Acceptance for the rest: no starter page
tells an in-checkout reader to `cargo build` in a leaf without saying the recipe
is the in-tree path.

**W6 — dead relative links, and a gate for them. LANDED.** The audit found nine
relative links in the book resolving to nothing. Two were archival (`phase-115`,
`phase-212` moved under `archived/`); seven were depth errors — `../../docs/…`
written from `book/src/<dir>/page.md`, which is `book/docs/…`, one level short
of the repo root. All nine fixed; 468 relative links across 108 pages now
resolve.

`check-doc-refs` was green throughout, correctly: it resolves the numbered
series by ID and deliberately accepts an archived file, because the id is what
the reference means. A rendered link is a PATH, and no gate asked that question.
`scripts/check-book-links.py` now does, wired into `check-fast` (buildless —
reads tracked files, resolves paths, never invokes mdbook). Self-tested: a dead
link makes it exit 1, and the page restores byte-identical.

**The gate's own first version had the defect it exists to catch.** It matched
only `./` and `../` prefixes and reported "469 links OK" — while skipping every
bare-filename link in the book, of which there are plenty
(`](custom-platform.md)` and `](custom-board.md)`, six each). A green whose
scope is narrower than its message is exactly what `check-fast`'s own
"1 check(s) did NOT run … this green is narrower than it looks" footer exists to
prevent. Widened to every non-external target: 525 links now, all resolving, and
the self-test covers BOTH spellings.

Note for W4: `mdbook` is not installed on this host and `just book` additionally
fails on a pre-existing rustdoc intra-doc link error in `nros` (`no item named
'std' in scope`), unrelated to this phase. The link gate is what verified the
book here; a real render has not been done.

**W7 — the rest of the currency sweep. LANDED.** Auditing what the book tells a
reader to *type*, beyond the sync step, against what exists:

* **Three `just` recipes that name nothing.** `just test-nuttx` (→ `just nuttx
  test` / `test-all`), `just test-qemu` (no such recipe — the serial page now
  names the actual test, `cargo nextest run -p nros-tests --test emulator
  test_qemu_serial_pubsub_e2e`, with the skip-counts-as-failure caveat), and
  `just nuttx build-fixtures-make`, retired by phase-212 M-F.16 in favour of
  `scripts/nuttx/stage-external-apps.sh --bringup <dir>` — the justfile says so
  in a comment where the recipe used to be. Checked all 293 `just` invocations
  in the book; the rest resolve (the noise is English prose — "just works",
  "just a", "just like").
* **A stale artifact path in `reference/build-commands.md`.** It pointed the
  manual QEMU test at `examples/qemu-arm-baremetal/rust/talker/target/…/release/`.
  Since phase-340 P2 that build lands in `build/cargo-fixtures/<group>/` under
  the `nros-relwithdebinfo` profile. Both the directory and the profile were
  wrong, and following it is also what produces the W5 residue. The page now
  gives the current path AND says it is computed, naming
  `nros_fixture_row_artifact_dir` as the SSoT rather than inviting the next
  literal.
* **A porting instruction for a crate that was deleted.**
  `porting/custom-platform.md` told a porter to add a `myos = []` ABI marker to
  `packages/rmw/xrce/xrce-sys/Cargo.toml`. That crate was deleted in phase-321
  W1.d — the directory is now a submodule host whose README says "do not re-add
  a crate here" — and phase-129.C.1 had already deleted XRCE's whole
  `platform-<rtos>` feature mechanism in favour of `target_os` selection. So the
  step was wrong twice over and would have had a porter re-create the forbidden
  crate. zenoh's half of the instruction is still correct and stays.
* **A sentence broken by a mechanical edit.** `internals/creating-examples.md`
  read "There is no `find_package(NanoRos)` path deleted it along with…" — the
  "strip Phase N references" commit removed the em-dash clause and left the
  remains. Checked that commit for siblings; this was the only one.

## Verification, and what it could not reach

`check-fast` GREEN (the lane every gate in this phase lives in), plus
`check-doc-refs`, `check-book-links`, `check-issue-index`,
`check-roadmap-status`. One check did not run and therefore verified nothing:
`check-abi-bindings` (bindgen-cli not installed) — irrelevant here, no header
was touched.

**Tier 1 could not be brought green on this host, for a cause this phase did not
create.** Filed as [issue 0696](../issues/0696-stale-verdict-names-an-input-that-is-not-one.md):
33 native C/C++ tests read STALE against `packages/testing/nros-tests/src/lib.rs`,
a file in none of their dep graphs, which `just native build-c` cannot clear
(exit 0, artifact mtime unchanged). With the bypass the message itself names,
every one of them passes.

The residue after that bypass is 6 reds and NONE is a real failure:

| test | what it is |
|---|---|
| `baremetal_board_run_executes_run_plan` | capability skip, rewritten by the junit pass |
| `entry_matrix` | capability skip, rewritten |
| `nano2nano::test_peer_mode_communication` | documented skip (issue 0682), rewritten |
| `case_18_cpp_xrce_action` | in-sweep flake — 2/2 solo |
| `zenoh_integration::two_sessions_…_router` | `skip!` for `ZPICO_MAX_SESSIONS=1` that the rewrite does not reach, because it is not an `nros-tests` suite (recorded on issue 0695) |
| `a_stale_verdict_reports_its_own_reasoning_and_its_age` | asserts the staleness verdict — the `NROS_SKIP_FIXTURE_CHECK=1` I set to work around 0696 is exactly what it is written to catch |

Also seen and retested rather than assumed: `nros_rmw_cyclonedds_ros2_srv_e2e`
failed once in-sweep (34.7 s) and passes 5/5 solo (7.0 s) — the host runs a live
88-process Autoware/CARLA stack contending for DDS discovery, which the
maintainer has already identified as environmental.

This phase's diff is markdown, one buildless Python gate, and one probe shell
script. None of the above is reachable from it.
