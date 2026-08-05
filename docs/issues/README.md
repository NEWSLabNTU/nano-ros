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

**#200** — fixture-build timing campaign blocked on a big-disk CI runner (phase-226 validation
residue). See `0200-*`.

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

**#418** — raw action feedback/result payloads carry an EXTRA CDR encapsulation header
(`[outer][goal_id][INNER][body]`), so they are wire-incompatible with ROS 2 *and* with nano-ros's
own typed path. Raw↔raw is self-consistent — every action Runtime cell pairs a raw server with a
raw client, so the double header cancels and nobody noticed. The inner header is deliberate (the raw
consumer reads the body with `new_with_header`), and removing it producer-only reproduces the
issue-#35 corruption its comment warns about, so producer and consumer must change together. This is
what stops phase-338 W3 migrating `action-{server,client}` / `service-client`. Decision in
[RFC-0069](../design/0069-action-payload-envelope.md). See `0418-*`. (2026-08-05)
(#416 resolved 2026-08-05 — `nros sync`'s source digest pruned build dirs by EXACT name, so it skipped
`target/` and walked straight INTO `target-tls` / `target-zenoh` / `target-xrce` / … , the isolated
build dirs `fixtures.toml` gives feature-variant rows. It then read every artifact it found, racing
cargo's own temporaries, and `just build-test-fixtures lane=native` died at a random fixture naming
an `rmeta`/`.o` path. RESOLVED by recognising a cargo build dir instead of listing names —
`CACHEDIR.TAG` plus a `target-` prefix for the not-yet-built case. Listing the dirs would have been
the hand-maintained-exclude-list shape issue 0287 already replaced. See `archived/0416-*`.)

**#422** — TRIAGE INDEX for the 19 runtime E2E failures on a clean tree (tier 1 gates all pass;
1231/1257 tests do). Eight are now diagnosed across three bugs — `0427` (stale SystemModel),
`0428` (cyclone node-register), `0429` (nano2nano grep drift) — and eleven remain. The original
"plausibly environmental" framing was WRONG and is corrected in the issue: zenohd/ROS are present
and every failure examined so far is a real defect. See `0422-*`. (2026-08-05)

**#431** — Every NuttX cell skips on a fully provisioned host: `NUTTX_DIR` is never exported by
activate.sh or the SDK env, and nothing provisions kconfig (and the printed `pip install kconfiglib`
remedy is refused on PEP-668 distros). `nros setup qemu-arm-nuttx` succeeds while every cell reports
SKIPPED. That is how #420 could assert broken behaviour that was never broken. See `0431-*`.
(2026-08-05)

**#433** — `just nuttx build-fixtures` exits 0 and the fixtures it just built read STALE: the kernel
is re-staged AFTER the entries link, so `staging/libc.a` is newer than the binary depending on it and
the freshness probe can never converge (measured: entry 20:42:48, libc.a 20:46:00). Two consecutive
green builds leave the same four cells unrunnable — the 0350 class, where a coordinate that never runs
looks like one that cannot. Blocked RFC-0069's last acceptance item for `nuttx c`/`nuttx rust`.
NOT the cause and already fixed: stale caches naming the board dir phase-337 W3 retired, and a
`--profile nros-minsizerel` argv-quoting bug. See `0433-*`. (2026-08-05)

Recently resolved (2026-08-05 cycle) — #248, #382, #392, #398, #411, #412, #413, #416, #419, #420, #421, #423, #425, #426, #427, #428, #429, #430. Their summaries live in `docs/issues/archived/`.
