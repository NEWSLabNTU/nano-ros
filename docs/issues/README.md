# nano-ros Issues

This directory tracks nano-ros bugs, limitations, and tech-debt — one file
per issue, mirroring the repo's numbered-RFC convention
(`docs/design/NNNN-*.md`) and the roadmap `archived/` pattern. Each file
carries YAML frontmatter plus the issue body (problem, evidence, current
state, fix/direction). Open issues live directly in `docs/issues/`; resolved
ones move to `docs/issues/archived/`.

## Conventions

**Frontmatter schema** (every issue file):

```yaml
---
id: 7                    # the issue number (matches the 4-digit filename id)
title: Unbounded message sequences capped at 64 elements
status: open             # open | resolved | wontfix
type: enhancement        # bug | enhancement | tech-debt
area: codegen            # codegen | rmw | memory | cmake | zephyr | nuttx | freertos | threadx | build | testing
related: []              # e.g. [rfc-0023, phase-218] — cross-links to RFCs / phases
resolved_in:             # (resolved only) commit short-hash or phase, e.g. "Phase 140"
---
```

**Lifecycle**:

1. Open an issue as `docs/issues/NNNN-slug.md` with `status: open`.
2. When resolved, set `status: resolved` + `resolved_in:` and **move** the
   file to `docs/issues/archived/NNNN-slug.md` (trimmed to a terse
   resolution summary). Do the move in the SAME commit as the status flip —
   skipping it is how 39 resolved issues accumulated in `docs/issues/` by
   2026-08-02, with the "Open issues" list below carrying 86 rows for work
   that was already done.
3. **"Open issues" below lists exactly the files in `docs/issues/`** — one row
   per open issue, no more. A resolved issue keeps a row only under "Recently
   resolved", and only for the current cycle; after that `archived/` is the
   sole record. Verify with: `ls docs/issues/0*.md` versus the `**#NNN**` rows.
4. **Numbering** = the next integer after the highest existing id.
   **Slug** = a kebab-case form of the title; the filename id is the
   zero-padded 4-digit issue number.

## Issue vs RFC vs phase doc

- **Issue** (`docs/issues/`) = a bug, limitation, or tech-debt item.
- **RFC** (`docs/design/NNNN-*.md`) = a design decision.
- **Roadmap phase** (`docs/roadmap/`) = an implementation plan.

Issues cross-link to the RFCs and phases that inform or resolve them via the
`related:` frontmatter field.

## Open issues

Recently resolved (2026-08-06): **#444** — every FreeRTOS **Rust** cell (pubsub, service AND action —
the issue's action-only scope came from one run and was wrong) booted, printed "Network ready." and
stalled forever. Reproduced on main with fresh fixtures, so not branch-specific. TWO faults, both from
the phase-338 W2 `-entry` collapse (`ab486a8db`). (1) The deploy block kept `tcp/10.0.2.2:7447` and no
`ip`/`gateway`, so lwIP came up on the board's STATIC 192.0.3.10 while the harness launched default
slirp — unroutable, and 7447 is a port the harness never serves; gdb put it in a blocking
`_z_open_link`. (2) One ZID for two peers: the platform PRNG is (ip, mac)-seeded because zenoh-pico's
ZID comes off it, both images booted the same pair, and the router keeps ONE peer (`max_links=1`) —
what `NROS_ENTRY_IP_LAST` 10/11 has always prevented for C/C++. RESOLVED upstream by `07faa2383`
(ip .15/.16 + gateway + per-variant port, restoring the default-slirp plan the launcher expects); an
independent fix from the other direction (move Rust onto the C/C++ board-net plan, unify the launcher)
was dropped in its favour. All NINE FreeRTOS cells pass. Still open underneath: `rtos_e2e` keeps the
Rust lane on a SEPARATE network plan from C/C++ on the same board, and its own comment calls unifying
them follow-up work — a lane whose firmware config and launcher are maintained apart is a lane where
they can silently stop matching. See `archived/0444-*`.

Recently resolved (2026-08-06): **#457** — a leaf `.cargo/config.toml` is a hand-kept COPY of `nros-board.toml`'s `cargo_config`, not
user-specific content. Measured: of 46 tracked leaf configs under `examples/`, **39 are deploy-bound**, and
NONE patches a generated msg crate — what they carry (`[build] target`, `build-std`, linker rustflags,
`[env]`, board path patches) is declared verbatim in the board descriptor. `check-board-cargo-config-applied`
states the contract: "the leaf file is TRACKED and `nros sync` leaves it alone, so the two are kept in step
by hand" — and that gap caused BOTH 0440 (~3680 undefined libc refs) and 0444's sibling failure. Proposal:
`nros sync` renders it and it gets gitignored, like `nros-patch.toml` and SystemModels already are; the 7
non-deploy-bound leaves decide whether the tracking gate inverts or just narrows. See `0457-*`. (2026-08-06)

**#450** — the group-A `action-server` example body publishes a FIXED `[0, 1, 1]` instead of computing the
sequence, and phase-338 W3.d's convergence deleted the riscv64 copy that did the real iterative Fibonacci.
Safe at the time (that cell is `BuildOnly`) but backwards: `Fibonacci` is the canonical ROS 2 action demo
BECAUSE the sequence is computed and streamed as feedback, and the goal's `order` is read, logged, then
ignored. Growing the group body touches four platforms with live runtime lanes and raises feedback buffers
128 → 256 on constrained targets. See `0450-*`. (2026-08-06)

**#451** — the embedded SDK env vars live only in `just/sdk-env.just`, so a direct `cargo build` of an
embedded example fails one variable at a time (5 freertos, 4 threadx, 1+ nuttx) even though every SDK sits
at the default path. The failure does not look like a missing variable: `zpico-sys` panics inside a
dependency build script, and a partially-configured NuttX build reaches the LINKER with `undefined
reference to open / socket / malloc` — mistaken for a link regression during phase-338. CLAUDE.md names
`activate.sh` the env SSoT; for these it is not. Same shape as 0407 and 0420's real finding.
See `0451-*`. (2026-08-06)

**#452** — embedded builds regenerate `nros_generated.h` / `nros_cpp_ffi.h` with an OLDER cbindgen, so any
embedded lane silently dirties two tracked headers, and committing it REVERTS the C23 enum-base guard (had
to be hand-reverted twice during phase-338). The repo already pins `clang-format` and `bindgen-cli 0.72.1`
for the C→Rust direction for exactly this reason; the Rust→C direction has no pin.
See `0452-*`. (2026-08-06)

**#445** — a staleness verdict is TERMINAL and self-explaining, so it
absorbs whatever the fixture would have done at runtime. Demonstrated: issue 0442 made the
freertos/threadx C/C++ cells read stale; fixing it made them run and immediately exposed issue 0444, a
real FreeRTOS Rust runtime failure that had been sitting behind it. I hit the reader-side half too —
asked why those cells were stale I gave a plausible, consistent, WRONG answer, because the verdict
explains itself well enough to survive scrutiny. RESOLVED without weakening any probe: the exemption
rule now has ONE spelling (`fixtures::staleness::exempt_probe_input`, which 0442's own fix had left
divergent on a THIRD arm), every verdict prints what it examined and exempted, and a per-coordinate
ledger counts consecutive non-running resolutions — from the second, the message says the runtime
result is being absorbed. `just fixture-staleness` lists them; `check-staleness-probe-exemptions`
rejects a second spelling or a probe that does not account, report and clear. See `archived/0445-*`.

Recently resolved (2026-08-06): **#442** — the regenerated-header exemption was applied on the ninja
dep-info arm of the cmake freshness probe but NOT on its sibling directory-walk arm, so every
freertos / threadx-linux C and C++ zenoh fixture read STALE against `zpico-sys/c/include/zpico.h` —
a cbindgen header written in place, whose mtime moves on any other feature set's build with the
content unchanged (measured: header 23:46 vs binary 21:23, `git status` clean). Issue 0196 one layer
in. RESOLVED: `newest_source_after` skips it like the loop does; action cells went 3 → 7 of 9 passing.
First written off as "a core-crate change staled main-built fixtures" — plausible and wrong, since the
observable is identical; reading the actual `newer:` path settled it. See `archived/0442-*`.

Recently resolved (2026-08-06): **#440** — phase-338 W2's `-entry` collapse kept the NODE package's
`.cargo/config.toml` and dropped the board's STATIC link args, so all six NuttX Rust entries failed
with ~3680 undefined libc references (`grep -c lsched` was 0 of 6). RESOLVED: the 24 args restored
from `nros-board.toml`'s `cargo_config` (the SSoT, not the deleted files), rustflags only —
`build-fixtures-arm` RC=0 with 0 undefined refs and all three nuttx action cells (Rust/C/C++) pass.
Gated by `check-board-cargo-config-applied`, watched to fire. Nothing caught it because the broken
config was valid TOML that cargo and `nros sync` both accepted — the loss showed only at link time,
on one platform. See `archived/0440-*`. (2026-08-06)

RESOLVED 2026-08-06 — **#439** a lane-narrowed build killed any recipe naming a fixture by `--id`, so
three of eight tier-2 modules died, no stamp was written and `just ci-matrix` could not run at all.
Two guards each right alone: #0393 removes rows for LANE reasons, #0406 treats a zero-row `--id` as a
typo — together, a lane dropping the row made 0406 blame the caller, printing requested and declared
coordinates that were IDENTICAL because the lane appeared in neither. Fixed in `9c6420144` (re-query
without `--coords-from`: present ⇒ out-of-lane, exit 0; absent ⇒ real typo), closed here after
verifying all three reported invocations exit 0 AND that a genuine typo is still fatal with and
without a lane — the failure to fear was a guard gone lenient generally rather than lenient about
lanes. See `archived/0439-*`.

RESOLVED 2026-08-06 — **#443** the staleness gate ignored the run's lane, because the lane is two env
vars and `ci-matrix` set only one: `NROS_FIXTURE_SCOPE` fell back to `all` and the gate audited the
whole tier-3 set while the run, build and stamp were tier 2. `_check-fixtures-stale` now DERIVES the
scope from `NROS_FIXTURE_LANE` (verified: `scope=coords (lane:tier2) … 13 coordinate(s)`), so
`ci-matrix`, `ci-matrix-nightly` and the next lane recipe need no second variable. Closed with the two
pieces derivation alone does not give: the gate now PRINTS the scope it chose and where it came from
(the defect survived because a green line looks identical whether it covered 3 coordinates or 47 —
issue 0445's lesson on the build side), and an explicit scope that CONTRADICTS the lane now exits 2,
which closes the dangerous direction the issue named (a scope narrower than the lane launders a green).
`just ci` and `_lane-gate` unchanged; `NROS_TEST_SCOPE` stays independent by design. See
`archived/0443-*`.

RESOLVED 2026-08-06 — **#437** `check-fast` was RED on main: `check-build-profile-literals` flagged
four sites in `just/px4.just`. The PX4 SITL lane is deliberately release-only (the FFI archive and the
symbol fixture land in the SAME image; a split link takes half of each), so all four carry
`# profile-literal-ok: symbol fixture`. The PLACEMENT is the lesson: the window is 3 lines and the
flagged token sits on a `\`-continuation line where no comment can precede it, so the marker must be
inline or the LAST comment line before the command — a rationale block between the two puts it out of
range, which is how this failed twice, once while being fixed. Still uncoupled underneath: line 168
builds the archive and line 181 hands PX4 its PATH from separate spellings, so a profile move makes
them disagree and PX4 links a stale archive. See `archived/0437-*`.

RESOLVED 2026-08-06 — **#436** the PX4 uORB→RMW bridge now works end to end (uORB samples translated to CDR px4_msgs and published on zenoh, 100+ forwarded). Five defects fixed: TWO copies of zenoh-pico in one image (each with its own statics — z_open failed silently; zenoh-pico now has ONE source, the umbrella built with a platform feature); uORB registered under the deprecated unnamed shim so it could never be named; `open_multi`'s extra sessions were anonymous; two incompatible executor handles behind one `void*` (fixed by `nros_cpp_init_multi`, which opens multi-RMW sessions into a real CppContext); and four collapsed error seams. See `archived/0436-*`.

Recently resolved (2026-08-05): **#434** — FreeRTOS C++ TUs resolved `<nros/nros_config_generated.h>`
to the in-tree `#error` stub, so `freertos cpp` fixtures could not build. NOT the ordering race the
first diagnosis claimed (lifting the Zephyr-guarded `OBJECT_DEPENDS` changed nothing, and two builds
with the headers present failed identically): the include LIST order was wrong. phase-337 W5.b added
the SOURCE `packages/api/nros-c/include` to `FREERTOS_STARTUP_INCLUDES` for `app_config.h`, assuming
"the per-app generated header shadows this one" — it is shadowed BY it, landing at position 9 ahead
of the generated dirs at 10/13. Removed; the dir is still reachable as nros-c's INTERFACE include
(position 14). All three freertos action cells now pass. See `archived/0434-*`.

Recently resolved (2026-08-05): **#433** — `just nuttx build-fixtures` exits 0 and its fixtures read
STALE. Root cause: arm and riscv share ONE configured kernel tree, and the riscv half re-stages it
after the arm entries link (one run shows two full `Building NuttX...` plus two "up-to-date" skips).
Decisive test: `build-fixtures-arm` alone leaves the entry FRESH and all three nuttx action cells
pass. Root-caused and worked around, NOT structurally fixed — a per-arch tree or an arch-keyed
freshness signature is larger and belongs with the NuttX board owner. See `archived/0433-*`.


**#200** — fixture-build timing campaign blocked on a big-disk CI runner (phase-226 validation
residue). See `0200-*`.

Recently resolved (2026-08-06): **#435** — filed as "CMake fixtures do not depend on generated RMW
headers"; SUPERSEDED by **#442**, whose diagnosis is the correct one. Not a missing dependency:
`zpico.h` is generated IN PLACE by its own producer's build script, so it cannot be its own input.
The real defect was that `cmake_dep_info_newer_source`'s two arms disagreed — the ninja `-t deps`
loop skipped `REGENERATED_INPLACE_HEADERS`, its sibling walk did not — so the walk reported exactly
what the loop was written to ignore. Fixed in `2e333c068`. See `archived/0435-*`, `archived/0442-*`.

**#432** — the pinned `zephyr-lang-rust` (404fcef) cannot build the `zephyr` crate for ANY board
whose devicetree has gpio nodes: its DT generator emits a five-argument `GpioPin::new` against a
six-argument signature (`pin` without `dt_flags`). `CONFIG_GPIO=n` makes it worse, not better — the
generator reads the devicetree, and the `gpio-keys` augment carries no `cfg:` key, so the calls are
still emitted while the `raw` bindings vanish (14 errors instead of 4). Invisible until phase-337
W2.b added the first non-native_sim Zephyr board, native_sim having no gpio nodes. Since essentially
every real board has gpio, Rust-on-Zephyr is native_sim-only until this is fixed upstream; C and C++
are unaffected (no `zephyr` crate), which is why W2.b's cells build the C entry. See `0432-*`.
(2026-08-05)

**#259** — derived scheduling is quantitatively INERT: the model carries no per-callback WCET
(`MapperPath.exec_ms` is `None` everywhere), so the budget dim short-circuits, blocking (`B_i`)
can never be numeric, and the feasibility check assumes `B_i = 0` — **unsound whenever callbacks
share a resource**. The originally-filed symptom (`placement`/`non_preempt_scope` hardcoded
`NotRequested`) follows from it: both are MECHANISMS, not requirements, so no contract fact
implies them. Rewritten 2026-07-26 with the design review — contention is already declared via
MutuallyExclusive callback groups (no new vocabulary for the intra-node case), a `holds:` contract
field is rejected (locking is implementation, not interface), and `criticality → core pin` is
rejected outright (unfalsifiable adjective; would look like a partitioning claim nothing backs).
Prereqs: WCET fact, board peripheral registry, `SchedCaps` core count. See `0259-*`.
(phase-296 W5.11 2026-07-24; reframed 2026-07-26)

**#260** — the RFC-0052 Native sched dims are e2e-verified only on the FALLBACK arm: every realtime
fixture is uniprocessor, so the SMP core-pin ACCEPT path (`k_thread_cpu_pin` /
`pthread_setaffinity_np` / `vTaskCoreAffinitySet`, `#ifdef CONFIG_SMP`) is compile-verified only.
Needs ONE SMP fixture (a separate zephyr native_sim SMP variant, not the shared image) to flip a
two-mode core-pin e2e to accept. See `0260-*`. (phase-296 W5.11 2026-07-24)

**#271** — Orin SPE BTCM footprint regressed ~+195 KB between `d9af52be` and `21a3a4248`; a
minimal `Executor::open`+spin image no longer fits 256 KB. See `0271-*`.

**#362** — phase-325 W3's uORB→RMW bridge is blocked on TYPES, not plumbing. The plumbing is proven
(one PX4 module links uORB + zenoh, `NodeBuilder().rmw()` gives two sessions, backend selection is
the cargo-feature knob one layer down). But the bridge must TRANSLATE: inward a payload is the PX4
struct keyed by `ORB_ID`, outward a real ROS 2 subscriber needs CDR with a type name AND type hash.
`nros generate-px4-msgs` already emits exactly the right message set — CDR `px4_msgs::msg::*` from
the PX4 `.msg` tree — but as a **Rust crate**, for the XRCE companion path; an in-firmware module is
C++. Hand-rolling the CDR is not the shortcut it looks like: `rmw_zenoh` keys discovery on the type
hash, so a guessed one is either invisible to ROS 2 or, worse, decoded as a different type. Worth
doing beyond the bridge: using `px4_msgs` makes the bridge's ROS-2-facing contract identical to what
`uxrce_dds_client` already publishes, so a subscriber cannot tell which produced a sample — that
indistinguishability is the interop claim. Blocks W3 only; the W2 direct demo needs no CDR at all,
which is the point of it. See `0362-*`. (2026-07-31)

**#371** [severity **high**] — native_sim cyclone app abort()s at a near-deterministic 19–21 s
joining the full Autoware graph during the safety-island demo's scenario init (7/7 on 2026-08-01;
the same tree passed 2× on 2026-07-31 when one sim node was down). mrm_handler flaps
operate/cancel (hot service path) right before death; unnamed cyclone pthread; gdb masks it;
strace -k unwinds only to the zephyr print shim. Isolation: island alone / single-peer feed /
availability flap / idle sim / EKF-odometry all survive — only the full graph + scenario churn
kills it. TRIGGER CONFIRMED by A/B: shadowing autoware_manual_lane_change_handler out of the sim flips 7/7 aborts to VERDICT PASS. See `0371-*`. (2026-08-01)

**#374** (filed as #373; renumbered) — `nros setup native --rmw zenoh` source-builds zenohd
(`[tool.zenohd]` has no `dist.linux-x86_64`; assets never seeded) and pulls a SECOND rust toolchain
(`1.85.0`, zenoh's own pin) for 792 MB of store — while installation.md:123 promised "ships prebuilt
toolchains per platform per RMW" for exactly this board. **Narrowed 2026-08-01:** the wait is now
announced up front by `nros setup` and the book no longer promises unconditional prebuilts; what
remains is out-of-repo — publish `1.7.2-nros2` assets on `nano-ros-sdk` so the dist rows return.
See `0374-*`. (2026-08-01)

**#403** — the WCET bench emits prose nothing parses, and a QEMU run with a dead cycle counter reports
zeros as if measured. `nros-bench/wcet-cycles-qemu` prints `min=/max=/avg= cycles` to a semihosting log
with no parser, no schema and no consumer, in the `debug` group so no lane runs it. On QEMU the DWT
never increments: the bench detects this, prints a NOTE, then measures anyway and exits 0 — warning and
data on the same stream, and only the data is machine-shaped. Same failure as 0259 one layer earlier
(a non-measurement entering as zero, the most optimistic WCET there is). Direction: structured artifact
with conditions recorded, and a dead counter is a HARD failure emitting no numbers at all. See
`0403-*`. (2026-08-03)

**#404** — no schema for DECLARING a measured WCET. `MapperPath.exec_ms` is `Option<f64>` and nothing
outside rlm's own tests ever sets it, so rlm v0.1.4's `ChainFeasibleWithoutWcet` (issue 0259) now
reports missing evidence with nowhere to put it. Open questions: keying (board id / platform family /
named measurement profile — a WCET belongs to a context, not to code), unit (mapper wants ms, the bench
measures cycles, converting needs a clock rate), granularity (mapper wants a whole callback, the bench
measures primitives), and provenance/staleness. Blocked on 0403 — designing the schema before a
producer emits an artifact answers the keying question by guess. Invariant: absent stays representable
and stays the DEFAULT, else zeros get written in by hand. See `0404-*`. (2026-08-03)

**#415** — `nros::main!` picks the framework emit shape from a hardcoded **deploy-string** table,
while `nros ws check` reads `[package.metadata.nros.board] framework` off the board crate. Two
spellings of one fact, and the macro's falls through to `OwnedSpin` on an unknown key — a silently
wrong entry shape, not a diagnostic. Invisible until phase-337 W7.a deleted `embassy-stm32f4`, the
only in-tree key that selected `Framework::Embassy`; `Framework::Rtic` is in the same shape and only
still reachable via `rtic-mps2-an385`. Fix = let the macro read the same manifest key (needs the
expansion-time fs round-trip `rtic_board_spec_for` already defers). See `0415-*`. (2026-08-04)

RESOLVED 2026-08-05 — **#418** raw action feedback/result carried a SECOND CDR header, breaking raw↔{ROS 2, typed} decode. RFC-0069 (option A) made the producer header-less; the `nros-node` `payload_has_cdr_encap` value-sniff (a 2nd instance of #35) was deleted (splice unconditionally); C/C++/ffi audited clean. Verified: `action_envelope_tests` 3/3, a native typed-client ↔ Node-class server pair decodes result `[0,1,1]`, and `ros2 action send_goal --feedback` returns SUCCEEDED. 14/18 action cells green; the 4 remaining are blocked by build defects that predate this (freertos sizes-header / nuttx #0433), tracked under #0433 + #0422. See `archived/0418-*`.
(#416 resolved 2026-08-05 — `nros sync`'s source digest pruned build dirs by EXACT name, so it skipped
`target/` and walked straight INTO `target-tls` / `target-zenoh` / `target-xrce` / … , the isolated
build dirs `fixtures.toml` gives feature-variant rows. It then read every artifact it found, racing
cargo's own temporaries, and `just build-test-fixtures lane=native` died at a random fixture naming
an `rmeta`/`.o` path. RESOLVED by recognising a cargo build dir instead of listing names —
`CACHEDIR.TAG` plus a `target-` prefix for the not-yet-built case. Listing the dirs would have been
the hand-maintained-exclude-list shape issue 0287 already replaced. See `archived/0416-*`.)

**#455** — CLI unit tests share a FIXED `/tmp/nros-cli-core-tests/` scratch path:
`CARGO_TARGET_TMPDIR` is set for INTEGRATION tests only, and `check-cli-tests` runs `--lib`, so the
fallback branch is what every run actually takes. Two concurrent runs (second checkout, parallel agent
session, CI beside a local run) then race one directory — one exec's the stub `idlc` it wrote while the
other truncates it (`Text file busy`), and `remove_dir_all` deletes the other's scratch. Reads as a
codegen regression, passes 3/3 solo. Fixed in the two sites carrying that exact idiom (which shared a
base name with each other); ~9 other CLI scratch paths lack a pid and want one shared helper rather
than a tenth spelling. See `0455-*`. (2026-08-06)

**#448** — XRCE action client: `accepted=true got_feedback=false`, deterministic over 3 retries. The
goal request/reply AND the terminal result both arrive (the wait for `Result received:` succeeded, so
the output is non-empty) — only the feedback assertion fails, and it demands the literal
`"Next number in sequence received: [0, 1"`. Either the feedback TOPIC is broken while both services
work, or the grep encodes a payload prefix that changed (the #0429 / archived 0157 class, which has
already caused two false diagnoses here). Rule out the grep first. See `0448-*`. (2026-08-06)

**#447** — `realtime_tiers` native/rust: the 10 ms high tier publishes NOTHING (`ctrl_max` 0, which is
`unwrap_or(0)` = nothing parsed) while the 100 ms low tier delivers 5 samples and anchors the test.
Distinct from #0438 in the same file, which is a boot-path marker assertion — this fails after the
binary is running. Four candidates (tier not scheduled / bound wrong / observer starved / parse
failure) are not yet separated; the failure message prints counters but not the text they came from,
so dumping the `/ctrl` output is the first step. See `0447-*`. (2026-08-06)

**#446** — the same crate is compiled ~21x across leaf target dirs: 106 `nros-core` rlibs, **5**
distinct `-C metadata` identities (45 of them the same compilation). Measured the exact
incompatibility factors — profile, feature set, RUSTFLAGS, and **explicit `--target` even for the
host triple** (so corrosion builds can never share with plain cargo builds). Separately,
`incremental = true` keeps the identity but destroys byte-reproducibility across dirs
(CGU session token), which blocks any content-addressed reuse; phase-336 made that profile the
default everywhere. Isolation is applied per-DIRECTORY while incompatibility lives per-IDENTITY, and
those are not the same partition. sccache's role is UNVERIFIED — measure before acting. See
`0446-*`. (2026-08-06)

**#441** — `zero_copy::test_zero_copy_message_info` observes nothing: the demo listener emits neither
`"Waiting for"` nor `"seq="` (verified running the fixture directly — session opens, subscriber
declares, zero matches). Grep-drift like #0429, but NOT fixable the same way: #0429 retargeted at the
PUBLISHER shim, while this test's subject is the RECEIVE-side zero-copy trampoline, so pointing it at
the talker would keep it green while it stopped testing zero-copy. Worse, the fixture has no
`cfg(feature)` at all — `unstable-zenoh-api` only propagates to `nros`, so the zero-copy and plain
listeners print identically and there is nothing to assert on. Needs a receive-side channel (or an
in-process assertion) before the test can mean anything. See `0441-*`. (2026-08-06)

**#438** — `native_orchestration_tiers` (x2) grep a `multi-tier` marker that only the NUTTX board
emits; the native/linux board prints the generic `NullNodeRuntime` fallback instead
(`nros-board-linux/src/lib.rs:334`), so a native multi-tier binary can never satisfy the assertion.
Marker added to nuttx in `f28ebc379` and never mirrored. Same class as archived 0157/0164 — greps
want `nros_tests::output::*` constants, not literals. Decide whether the board should say it or the
test should ask differently. See `0438-*`. (2026-08-06)

**#422** — TRIAGE INDEX for the runtime E2E failures. **10** on freshly rebuilt fixtures (tier-1
gates all pass, 1242/1259 tests do). An earlier run said 19 — nine of those were STALE FIXTURES
after a rebase touched `nros/src/node.rs`, so rebuild the lane BEFORE triaging or you are measuring
the rebase. Diagnosed: `0427` (stale SystemModel, verified fixed), `0429` (nano2nano grep drift,
re-verified on fresh fixtures); `0428` turned out to be #0413's cyclone descriptor bug, resolved
upstream. Eight remain, listed with their messages. See `0422-*`. (2026-08-05)

RESOLVED 2026-08-05 — **#431** NuttX cells skipped on a host that ran only `nros setup qemu-arm-nuttx`. (1) `NUTTX_DIR` is in fact exported by `sdk-env.sh` (verified clean-env) — the filing's claim was stale. (2) The real gap: no kconfig frontend, and the `pip install kconfiglib` remedy is refused on PEP-668 distros. `scripts/nuttx/build-nuttx.sh` now self-provisions kconfiglib into a repo-local venv (`build/nuttx-kconfig-venv`) when none is present — venv pip isn't PEP-668-blocked, no sudo. (3) `just nuttx doctor` already reports the state. So the cells now run instead of skipping. See `archived/0431-*`.


Recently resolved (2026-08-05 cycle) — #248, #382, #392, #398, #411, #412, #413, #416, #419, #420, #421, #423, #425, #426, #427, #428, #429, #430. Their summaries live in `docs/issues/archived/`.
