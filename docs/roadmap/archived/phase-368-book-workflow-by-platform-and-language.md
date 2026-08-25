# Phase 368 — What a reader actually runs, per platform and per language

**Status (2026-08-20).** COMPLETE — W1–W14 landed, and the code-gap
follow-ups the target design deferred are closed too. The clean-container
probe re-ran GREEN on the merged tree (PROBE=0: bootstrap → setup →
C leaf build → BOTH scaffolded workspaces publishing, no router, no ROS)
after two sessions' concurrent fixes merged.

Closed since the W1–W14 landing:

* **Tool-emitted `just` strings** — 15 emitter strings + 5 cmake `message()`
  sites now name `./scripts/bootstrap.sh` first with the contributor recipe
  as the alias; one board error prescribed `just setup-threadx`, a recipe
  that does not exist (→ `nros setup qemu-riscv64-threadx`). Gated:
  `check-emitter-just-spelling` in `check-fast`, which caught the dead
  recipe on its first run.
* **The `-DNROS_RMW` silent override** — a hard-coded `BACKEND` now WARNS
  naming both values, the file to edit, and the forwarding idiom the
  scaffold ships; a forwarding root honors the flag silently.
* **`nros doctor` without `just`** — degrades (native checks stand, skipped
  contributor lanes named) instead of erroring after its own checks passed.
* **Issues 0694–0696, 0699 resolved and archived.** 0696 (the unclearable
  STALE) and 0699 (deep-path NameTooLong) were each root-caused
  INDEPENDENTLY by this session's subagents and a parallel session, same
  causes, hours apart — the landed fixes are the parallel session's, the
  duplicates dropped in rebase. 0695's shell half landed upstream; this
  session landed half 2 (the zpico lane ran nextest bare, bypassing the
  skip rewrite; the junit path now follows `CARGO_TARGET_DIR`). First
  bypass-free tier-1 green followed: `Real failures: 0 / 0`.

**The zenoh-track probe LANDED (2026-08-20): `just probe zenoh`.** Same
clean-container discipline on `ros:humble` (the interop page's own
prerequisite) under `--rmw zenoh`: installation prereqs → bootstrap →
first-node-rust's zenoh-default build → then the verifier replays
`ros2-interop.md`'s three terminals — the page's apt prerequisite, its exact
router invocation, the nano-ros talker, and `ros2 topic echo
--qos-reliability best_effort` asserting `data: 'Hello World:` arrives.
PROBE=0: **the book's cross-stack delivery claim is machine-verified**, the
one assertion the cyclonedds quickstart track cannot make. Three runs to
green, two real finds:

* **`activate.sh` died under `set -u`** — ROS's `setup.bash` is not
  nounset-clean (`AMENT_TRACE_SETUP_FILES`, line 8), so any strict-mode
  shell or `set -euo pipefail` CI runner failed INSIDE the sourced file,
  reading as an activate.sh bug. Fixed at the real level: nounset is
  suspended around the ROS source and restored to whatever the caller had
  (POSIX `case $-`, bash+zsh). Not probe plumbing — a user-facing fix.
* The verifier is faster than the book's human: terminal 2 opens seconds
  after terminal 1 by construction, so the probe polls the router's port
  before dialing (a client fails fast on a closed port).

**The last two book deferrals LANDED (2026-08-20).** The 22 wrapped
contributor lanes moved to `internals/platform-lanes.md` (one page, eight
platform sections, fences byte-identical; each source page keeps a one-line
pointer). And `esp32.md` got its user build spelling — not the ESP-IDF
rewrite first guessed, but the esp-hal path the page is actually about,
verified end to end in a copy-out: `nros sync` → `cargo +nightly build
--release` (nightly because the board config builds core/alloc from source)
→ `espflash save-image --chip esp32c3 --flash-size 4mb --merge` (334 KB app
in a 4 MB image). The Run section now names the one thing `nros setup
qemu-esp32-baremetal` does NOT provision — Espressif's QEMU fork
(`nros setup --tool esp32-qemu`, source-built; stock qemu-system-riscv32
has no esp32c3 machine) — and retires the page's pre-RFC-0075 claim that
setup lands a zenoh router. The boot line itself is documented from the
recipe verbatim; not run-verified here (the fork is not built on this
host — its e2e lane is deliberately optional), which is also why the
board's package list does NOT gain the fork: the tree treats the emulator
as an explicit opt-in, and the page now says so instead of implying it.

**The z3 feature-gate LANDED (2026-08-20) — nothing deferred remains.**
rlm v0.1.8 puts the checker's SMT satisfiability rule (the crate's only z3
consumer) behind a default-on `smt` feature; a no-smt build registers a
stub under the same rule id that emits one Info diagnostic whenever the
rule battery runs, so checking less is never silent. play_launch forwards
the choice; nano-ros's shipped `nros-launch-resolve` opts out — its
dependency graph carries ZERO z3, so `libclang-dev` and `libz3-dev` left
all four of the book's prerequisite spellings. Proven the only way this
phase accepts: the clean-container probe re-ran on the slimmed list,
PROBE=0, both scaffolded workspaces publishing. Every current consumer of
rlm/play_launch defaults is byte-for-byte unchanged (smt defaults on end
to end; verified 176/176 resolver tests in both states). The maintainer-approved TARGET DESIGN below defines
W8–W14 (restructure the book for user personas: C++-first workspace quick
start, cyclonedds default, no `just` on the user track).

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

**W4 — probe coverage. LANDED — and it PAID.** `just probe bootstrap` now
executes the QUICK START in a pristine ubuntu:24.04: the two-command install,
`nros setup native --rmw cyclonedds`, the C leaf's cmake build, then the
probe-owned verifier scaffolds BOTH workspaces (`nros new --workspace`,
`--lang rust`) and asserts each publishes with no router. Ten iterations to
green, and every red was a real defect in the flow the book documents:

| iteration | defect |
|---|---|
| 1 | `cmake` absent from the host-prereq block — the whole quick start is CMake |
| 2 | `zstd` absent — the cyclone prebuilt is a `.tar.zst` |
| 3 | the probe's own verifier asserted `zenohd` on PATH — false since RFC-0075; green only because nobody re-ran it |
| 4 | the front door never built `nros-launch-resolve`; first workspace configure died naming a `just` recipe |
| 5 | resolver build needs libclang (z3-sys bindgen); the front door's `\|\| return 0` swallowed the failure and smiled |
| 6 | z3-sys also needs the z3 HEADERS (libz3-dev) |
| 7 | the resolver links CPython — python3-dev |
| 8 | `[rmw.cyclonedds]` provisioned the C library but not rosidl; first type-support step died "rosidl_adapter is not importable" — the row now carries `rosidl` (the complete-provisioner contract) |
| 9 | rosidl_cli imports `yaml` — PyYAML joins `[python.*]` and the prereqs |
| 10 | **the linked libddsc had NO RPATH** — every dev machine loaded ROS's OWN libddsc via the sourced env's LD_LIBRARY_PATH, a different build than the one linked, working by coincidence; a ROS-less host died at exec. Both cyclone link sites now stamp the linked library's directory; verified with `env -u LD_LIBRARY_PATH` |

Also learned and recorded: the probe clones COMMITTED main from the mount, so
worktree edits to bootstrap/cmake are invisible until committed — fixes land
before their green run, not after. Out of scope stays as stated (hardware/
vendor-SDK/QEMU starters); the zenoh-track probe (first-node-rust.md's flow
under `--rmw zenoh`) is named follow-up, since that page left the chapter
list when the probe's rmw moved to the quick-start default.

*(superseded planning text below)* `probe=` covered 3 blocks on 2 pages, and
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


## Target design (maintainer-approved, 2026-08-20)

W1–W7 made the existing pages true. The rest of the phase restructures the book
around the people it serves, per three maintainer decisions and the evidence
each rests on.

### Decision 1 — the user track uses no `just`

`just` is a contributor dependency; a user has `nros`, their vendor's build
tool, and a shell. The tooling already agrees: `./scripts/bootstrap.sh` (no
subcommand) is THE front door and its own help says "no just required", then
prints the next step (`nros setup <board>`). Setup is therefore exactly two
commands, and stays two for every persona:

```sh
git clone --branch nros-v0.5.0 https://github.com/NEWSLabNTU/nano-ros.git
cd nano-ros && ./scripts/bootstrap.sh
nros setup native --rmw cyclonedds
```

### Decision 2 — cyclonedds is the quick-start default

RFC-0075: we ship no zenoh router — it is ROS's `rmw_zenohd`, so a newcomer
without a ROS 2 install cannot start one. CycloneDDS needs no daemon at all.
Verified end to end 2026-08-20: the C++ template with `BACKEND cyclonedds`
builds in two cmake commands and publishes at its 500 ms tick with NOTHING else
running. zenoh-pico (ROS interop) and XRCE (smallest footprint, serial) become
the alternatives one section over, where a reader who needs them has the
context they assume.

### Decision 3 — workspace-major, C++ first

Standalone-first was rejected because the config axes scatter and every
successful reader eventually pays a confusing restructure:

| axis | workspace shape | standalone shape |
|---|---|---|
| ROS edition | `[system] ros_edition` in `system.toml` | flag / ad-hoc |
| RMW | `system.toml` + one `BACKEND` word (C++) | cargo feature |
| board / target | `[deploy.<name>]` | `[package.metadata.nros.deploy.*]` |
| features | node pkgs + dims (RFC-0066) | cargo features |
| site facts | `[deploy.*]` (RFC-0072, landed) | env vars |

One authored home per axis in the workspace shape; the switch out of standalone
relocates all of them at once. Porters come from colcon and already think in
`src/` + packages; RFC-0072's landed design is workspace-shaped; RFC-0066
consolidated CI onto workspaces. Growth is monotone: add a package, never
restructure.

**C++ first** because the audience porting from ROS 2 is C++-background, and
because the C++ path measured simplest: copy
`examples/templates/multi-node-workspace-cpp`, `cmake -S . -B build
-DNANO_ROS_ROOT=…`, `cmake --build build`, run — no `nros sync`, no daemon,
and the RMW switch is ONE word (`BACKEND` in the root `nano_ros_workspace()`
call). Rust and C are walk-throughs added to the same, already-understood
workspace; `nros sync` (the #0694 trap) leaves the critical path entirely and
is introduced in the Rust chapter as "the Rust-side codegen step".

### The target spine

```
Introduction · Which Reader Are You?
# I   Quick Start — C++ & CMake, workspace-shaped
      install(2 cmds) · first project(C++ template, cyclonedds) ·
      anatomy(3 roles, one config file, "in rclcpp this was …" column) ·
      take-it-with-you(copy-out) · your own messages · troubleshooting
# II  Choosing an RMW
      why cyclone default · zenoh-pico(ROS interop + router story) ·
      XRCE(small/serial) · switching
# III Bring Your Own RTOS                        (RFC-0072/0003 track)
      how integration works(guest principle) · Zephyr/west ·
      NuttX/apps-external · ESP-IDF component · FreeRTOS(STM32Cube,
      MCUXpresso, Pico SDK named) · ThreadX · PX4 · IDE hosts(nros emit) ·
      bare-metal · QEMU sandboxes(demoted from "starters")
# IV  Growing Your Project
      add a package · params/lifecycle/QoS · more deploys ·
      Rust nodes(nros sync introduced HERE) · C nodes · mixed workspaces
# V   Coming From ROS 2
      setup-compared · differences · vs micro-ROS · migration ·
      porting a rclcpp node · interop
# VI  User Guide (task reference)   Concepts · Porting · Design ·
      Internals · Reference (unchanged shape)
```

Routing: hobbyist stops at I; an SDK owner searches their vendor's name and
lands in III; a porter reads I's anatomy then V — the same workspace shape
everywhere, so there is no shape switch to unlearn.

### Verified groundwork (2026-08-20), and the defects found doing it

* C++ copy-out template: builds and runs with cyclone, no router. Template
  fixes needed: ships `BACKEND zenoh`; `[deploy.native]` names no `board`
  (configure-time warning); README teaches the retired
  `MODEL config/system_model.yaml` spelling while the code is current.
* `-DNROS_RMW=` on the configure line is SILENTLY ignored when the root
  CMakeLists hard-codes `BACKEND` — the book must teach the CMakeLists edit.
* Rust copy-out template: sync → build green, but the RMW switch is THREE
  edits (`system.toml` `rmw`, the board dep's feature, the `nros` facade's
  `rmw-cyclonedds` type-descriptor feature) — the scaffold must bake all
  three from one flag. Also: the cyclone-switched entry exits `application
  complete` without ever ticking, where the C++ entry spins and publishes —
  unexplained, must be resolved before the Rust chapter teaches it.
* `nros new` has NO one-shot workspace verb: project mode is standalone,
  `system` mode scaffolds a bringup directory alone. Interim path is
  copy-out; the verb is W8.
* issue 0699 (filed): `nros sync` dies `Metadata(NameTooLong)` in a
  ~100-char-deep path; identical tree at 34 chars is clean.

## Work items — target design

**W8 — `nros new <name> --workspace`. LANDED.** One-shot minimal workspace
scaffold, `--lang cpp` (default) / `rust`, `--rmw cyclonedds` (default) /
`zenoh` / `xrce`. The file bodies are `include_str!` of the canonical
templates — one copy, no drift — and the RMW is a rewrite of known anchors,
each asserted to appear exactly once so a template edit that moves one fails
the scaffold loudly. Verified: scaffold → build → run publishes, BOTH langs,
no hand edits, no router. The `include_str!` freshness edge is closed:
`gen-cli-source-dirs.py` now folds parent-relative embedded-file directories
into the CLI stamp closure (the 0627 class, one layer down).

On the original acceptance's "CLI test asserting the scaffold builds as a
build-stage fixture": deliberately NOT done that way. Scaffold output ≡
template bytes + tested anchor substitutions; the templates themselves are
already built by the fixture lane and run by cpp_multi_node_entry's E2E, so
the only seam a new fixture would add coverage for is the substitution — which
6 unit tests assert directly. A second build of the same bytes buys latency,
not coverage.

**W9 — template repairs. LANDED.** C++ template defaults `BACKEND
cyclonedds` via an overridable `NROS_RMW` (the fixture lane pins
`-DNROS_RMW=zenoh` so cpp_multi_node_entry's zenoh E2E keeps its coordinate —
user default and test coordinate now differ ON PURPOSE, each spelled where it
belongs); `[deploy.native]` gained `board = "native"` in both templates;
READMEs and the entry doc-comment moved off the retired `MODEL`/`model =`
spellings. The Rust immediate-exit asymmetry RESOLVED: not an RMW bug —
the hosted `nros::main!` default is the `NROS_ENTRY_SPIN_MS`-gated bounded
spin (issue 0274), which exits right after registering when the env is
absent. The template entry now says `spin = "forever"`, and the talker/
listener gained `log::info!` lines (the linux board's stdout log bridge
prints them with zero setup) so a first run visibly ticks. Verified: fresh
Rust copy-out on cyclonedds prints interleaved Publishing/I heard at 1 Hz.
The Rust template's COMMITTED default stays zenoh (its E2E coordinate);
the scaffold bakes the user default.

**W10 — Part I, the C++ quick start.** New pages: first-project (verified
command transcript), anatomy (three roles, `system.toml` as the one config
file, rclcpp column), take-it-with-you rewritten around the C++ copy-out.
`installation.md` reshaped to the two-command setup with cyclone default.
Probe tags on the quick-start blocks so `just probe bootstrap` executes the
C++ path (extends W4).

**W11 — Part II, the RMW section.** `rmw-backends.md` splits: why-cyclone,
zenoh-pico + the router story (moves here from the starters), XRCE, and one
switching page per builder (the one-word C++ edit; the Rust flag once W8
bakes it).

**W12 — the de-just sweep. LANDED.** 139 inventoried `just` sites across 27
user-track pages: 36 REPLACED with verified user spellings (`./scripts/
bootstrap.sh` for the CLI build; `ZENOH_CONFIG_OVERRIDE=… ros2 run
rmw_zenoh_cpp rmw_zenohd` for every router; plain `nros doctor` /
`nros setup --check`; per-leaf cargo/cmake for fixture-recipe stand-ins;
`nros setup --source px4-*` for PX4), 24 contributor blocks wrapped in marked
asides (relocation to internals/ deferred, list in the sweep commit), the
rest marked in place.

Two findings beyond the plan, both now fixed in the pages:
* **Every router port the user track quoted was stale** — the book said
  7450–7456 where the recipes' allocator uses 7400–10200. Replacements
  re-derive from the recipe body or, better, use the port the example's
  deploy locator actually DIALS (bare-metal's "verbatim" TOML quote said
  7450; the real leaf says 10500).
* `nros doctor --platform <p>` and arm-fvp's `nros doctor --board` shell out
  to `just` (`run_just_doctor`), so the only just-free doctor surface is the
  plain form — a de-just CLI gap worth its own issue.

Gate: `check-book-no-just` in `check-fast`. v2 licenses contributor content
three ways (marker within 8 lines, case-insensitive; a "contributor" heading
licenses its section; a "contributors" table-header licenses its rows) plus
an explicit `no-just-ok` escape hatch for verbatim tool output. Self-tested
failing on an unmarked in-scope injection. Two tool-emitted strings still
SAY `just setup-cli` (the CLI's stale/missing message, a build.rs hint) —
quoted verbatim with the user remedy beside them; de-justing the emitters is
follow-up.

**W12 residue, recorded rather than smoothed:** `esp32.md`'s Build section
now contains ONLY a contributor aside — the page never had a user-side build
spelling, because the esp32 QEMU flow's build is genuinely the fixture lane
(espflash image packing + shared target dirs). The honest fix is the W13
rewrite of that page onto the ESP-IDF component shell (`idf.py build` is the
user spelling), not a synthetic bare command here.

**W13 — Part III, BYO-RTOS.** New "how integration works" page from
RFC-0072/0003 (guest principle, shell table, board pkg + `[deploy.*]`);
existing platform pages re-shelved under it and rewritten against the LANDED
shells (Zephyr/NuttX/ESP-IDF); FreeRTOS page documents today's
`nros_freertos_build_kernel` path while naming the direction; QEMU pages
demoted to a "try it without hardware" cluster. Vendor names (STM32Cube,
MCUXpresso, Pico SDK) appear as searchable headings.

**W14 — the new spine.** `SUMMARY.md` restructured to the target spine;
`choose-your-entry.md` rewritten to route the five personas; every moved page
keeps its path (mdbook has no redirects — re-shelving is a SUMMARY move, not
a file move) so external links survive.

Sequencing: W8+W9 first (the book depends on the scaffold), W10–W12 in
parallel after, W13 next, W14 last (the spine move lands when its sections
exist).

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
