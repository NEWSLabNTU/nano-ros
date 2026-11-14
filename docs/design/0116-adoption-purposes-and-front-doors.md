---
rfc: 0116
title: "Adoption purposes and front doors: three purposes as acceptance columns, an action-only user surface, serial-first chapters, and one run verb"
status: Draft
area: docs-ux-diagnostics
since: 2026-10
last-reviewed: 2026-11-14
implements-tracked-by: [phase-525, phase-523]
amends: []
supersedes: []
superseded-by: null
---

# RFC-0116 — Adoption purposes and front doors

**Status:** Draft (2026-10-31). Written from the maintainer discussion of
2026-10-31 on who adopts nano-ros and what stops them. Implemented by
[phase-525](../roadmap/archived/phase-525-adoption-front-doors.md) (the front doors on
QEMU, no hardware) and [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)
(the lab series and the adoption probes). Its two siblings are RFC-0115
(the emulator-first board policy, phase-524) and RFC-0114 (the board support
registry as the one primitive, phase-522); this RFC names them where a
decision leans on theirs and does not restate them.

## Summary

nano-ros is adopted for three purposes, and today's front doors serve none of
them well. This RFC names the three purposes and makes them the acceptance
columns of every user-facing surface (D1). It then fixes the four frictions
every purpose meets: the user surface speaks in internal ids (D2); the first
chapter asks for a network before a cable (D5); the first output needs six
concepts (D6); and running what was built takes a different command on every
road (D7). It closes with the method that finds the next friction before an
adopter does: adoption probes, one per purpose (D9).

Decision numbers are shared with RFC-0114 and RFC-0115 so that a work item can
name "D4" and mean one thing across the three documents: D3, D4 and D8 are
RFC-0115's; D10 to D12 are RFC-0114's.

## Problem

The findings below are the maintainer discussion's own words, kept as written
so the acceptance in phase-525 and phase-523 can be checked against them.

### Shared frictions, met by all three purposes

1. **Vocabulary on the user surface.** Phase, issue and RFC numbers appear in
   `nros --help`, in refusals, in `nros doctor` output and in getting-started
   chapters. Measured 2026-10-31 in `nros --help`: `setup` is described as
   "Resolve + fetch a board's toolchain/SDK packages (Phase 187)", `sdk-path`
   as "phase-365 W2 — print the CONSTRUCTED install path", `locate` as
   "phase-484 W1 (RFC-0103 D4)", `sdk-front` as "phase-431 W3", `sdk-root` as
   "phase-447 A2", `store` and `toolchain` as "phase-440 W6", `pin` as
   "phase-443 W2", `update` as "phase-485 W7 (RFC-0104 D7)", and so on through
   most of the verb list. A reader with no checkout cannot follow any of these.
2. **No hardware twins.** Nobody adopts on QEMU. Every in-tree board is a QEMU
   machine, a host simulator or a model; a reader holding a board finds no
   chapter for it and no emulator that behaves like it.
3. **Install weight and reach.** No offline or mirrored install; no Windows or
   macOS story; the Zephyr workspace alone is 2.5 GB without the module
   allowlist; time to first output is not measured or stated anywhere.
4. **Concept count before first output.** Workspace, bringup package, image,
   launch file, entry and `system.toml` all come before one publish.
5. **No single run verb.** `west build -t run`, a QEMU command line, a host
   executable path and `west flash` are four spellings of "run what I built",
   and only `nros build` knows which one applies.
6. **No per-board evidence page.** Which boards are measured, where, how
   often, and when they were last green is spread over the support tiers
   page, the support status page, the test matrix and the probe workflow.

### Industrial: safety and robustness professionals

Practices to expect: pinned toolchains and reproducible builds; air-gapped or
mirrored fetch; an SBOM and a licence audit of every vendored dependency;
static analysis (MISRA, clang-tidy, pedantic clippy); no heap after
initialisation; stated memory and stack budgets with evidence; worst-case
timing or at least jitter numbers; fault injection; requirement-to-test
traceability; long-lived pins with security backports; review of every patch
carried on a fork.

Frictions: evidence is scattered across ten documents; no per-board page says
what is measured, where and how often; the offline install story is unproven
(every probe track is online); fork deltas are documented for cyclonedds only;
refusal texts name issues and phases, not actions; `nros doctor` is not a
conformance report; and the question "who supports this in five years" has no
written answer.

### Field solutions: technicians building per-customer systems

Practices to expect: copy a working example, change a topic name and a
number, flash, and see data in `ros2 topic echo` or PlotJuggler. An Arduino or
PlatformIO mental model, a vendor IDE, a Windows laptop, serial `printf`
debugging. They will not read RFCs and will not learn cargo, cmake, west,
Kconfig, `system.toml`, bringup, images, launch or entry.

Frictions: our vocabulary; the Rust toolchain install; Linux only; QEMU-first
chapters when they hold a real board; `nros new` scaffolds a workspace with a
bringup package and a launch file; errors name internals; no sensor-node or
GPIO-actuator example; no one verb to build, flash and watch.

### Education: college and engineering-school labs

Practices to expect: an instructor builds one lab image per semester and 20 to
60 students repeat it on shared or personal laptops, often Windows or macOS,
often behind a university proxy. Boards are what the department bought in
bulk. Grading needs reproducibility: the same toolchain on every seat, the
same output. The course teaches ROS 2 concepts (topics, QoS, services,
lifecycle) and embedded concepts (tasks, memory, timing), not our build system.
A lab is two hours; install has to fit in fifteen minutes or be done beforehand
in a lab image. Students debug by pasting error text into a search engine.
Instructors want slides, exercises with known answers and automatic grading.

Frictions: install time and the 2.5 GB Zephyr workspace; no Windows or macOS;
no offline or mirrored install; versions drift mid-semester (pins exist, but
`nros pin` is not taught); no exercises; concepts named by our internal
vocabulary; no expected output per step beyond the `probe-expect` blocks.

What education already has: the QEMU-first chapters are right for a lab with
no hardware, and the probe tracks prove them weekly.

## Prior art in the tree

- [phase-517](../roadmap/archived/phase-517-user-flow-setup-build-flash.md)
  built the book probes: every QEMU chapter replays weekly in a pristine
  container, each Run step asserted against the chapter's `probe-expect`
  block. The probes prove the mechanics of a chapter; they do not ask whether
  a reader understands it, and they do not time it.
- [phase-452](../roadmap/archived/phase-452-copy-out-front-door.md) made what a new user
  copies buildable by a lane (`check-template-copy-out`), and settled that
  in-tree workspaces are copy-out.
- [phase-474](../roadmap/archived/phase-474-safety-island-board-findings.md) and
  [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)
  are the one real adoption trace the tree owns: the Autoware safety island on
  an NXP S32K344, with the adopter's own developer-experience gap list. Its
  findings are the industrial purpose's seed.

## Design

### D1 — Three purposes, three acceptance columns

Every user-facing surface (a getting-started chapter, a CLI verb's help and
refusals, `nros doctor`, the evidence page) is accepted against three columns,
each with a stated budget:

| Purpose | Who | Budget | Measured by |
| --- | --- | --- | --- |
| **Industrial** | safety and robustness professionals | reproducible, auditable, offline-capable: the same inputs give the same image, every fact is on a page, and the install runs with no network | the offline install probe; the evidence page; `nros doctor --report` |
| **Field solutions** | technicians with entry-level programming building per-customer systems | 30 minutes from an unopened box to `ros2 topic echo` showing the board's data | the first chapter's probe, timed |
| **Education** | college and engineering-school labs | 15-minute install (or a prebuilt lab image), a lab that fits two hours, the same result on every seat | the lab series' install probe and per-lab grading command |

A purpose column that a surface cannot serve says so in the chapter ("this
chapter assumes a Linux host; the Windows path is …"), never silently.

The word for these is **purpose**, not persona: the columns describe what the
adopter is trying to do, which is what the acceptance checks, not who they
are.

### D2 — The user surface names an action, never an internal id

What holds now: every user-facing surface names what happened, where, and the
next action, and never a phase, issue or RFC id. That covers every page of the
user book, `nros --help` and every verb's help, `nros doctor` output, what a
successful build prints, and every user-reachable refusal, build-stopping ones
included. Ids live in source comments, `-v` detail, and a book page's closing
`## Design background` section, the only place a user page names an RFC.

Authoritative homes:

- The book's rule: [RFC-0124](0124-book-user-guide-organization.md)
  Principle 5, with the gate's scope (the whole user book, no contributor
  exemption) from [RFC-0123](0123-two-books-by-audience.md) D3.
- Refusals and build output: user ruling A of 2026-11-12, recorded in
  [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md) W5 and
  [issue 2305](../issues/2305-user-tier-messages-cite-internal-issue-ids.md),
  enforced by the ratchets `check-user-surface-ids` and
  `check-cmake-user-output-refs`; runtime logs and the colcon plugin are
  [issue 2443](../issues/archived/2443-runtime-log-and-colcon-messages-cite-ids.md).
- Stable diagnostic codes (ruling C): phase-552 and its diagnostic-code RFC,
  named in phase-523 W5.

### D5 — Serial first, ethernet second, WiFi stated as deferred

What holds now: the first path on a board opens over USB serial, ethernet
comes second on a board that has it, and WiFi is stated as deferred rather
than left for the reader to discover. Authoritative home:
[RFC-0124](0124-book-user-guide-organization.md) Principle 12. The scaffold
puts a board's first image on its serial port where the board states one
(`[board.net] host_serial`, [RFC-0121](0121-workspace-first-project-layout.md)
D4).

### D6 — A single-package front door before the workspace vocabulary

What holds now: there is no single-package front door. Every project is a
workspace ([RFC-0121](0121-workspace-first-project-layout.md) D1), and
`nros new` writes a node package and its bringup (RFC-0121 D4). What D6 kept
is the concept budget: the first chapter names no launch file, image or entry,
the reader authors nothing in the bringup, and the project grows by adding a
package. Authoritative homes: RFC-0121 D1, D4 and its "Amendments to other
RFCs" row for 0116 D6, and [RFC-0124](0124-book-user-guide-organization.md)
Principle 12 for the book.

### D7 — One verb runs what was built

What holds now: `nros run` builds the image if stale, then launches a host
process or an emulator and streams the console; `--expect` exits 0 on a match
and non-zero on timeout, so the chapter's line, the printed next step and the
probe's line are one line. On a hardware image `nros run` refuses and names
`nros flash --monitor`. Authoritative homes:
[RFC-0118](0118-target-execution-model.md) D2 for the verbs,
[RFC-0121](0121-workspace-first-project-layout.md) D5 for the target argument
(`nros run <node>`), and [RFC-0124](0124-book-user-guide-organization.md)
Principle 12 for the book.

History: D2, D5, D6 and D7 were rewritten as pointers on 2026-11-14 (phase-555
ruling N3). Their earlier bodies, banners and amendments are in the git log of
this file.

### D9 — Adoption probes, one per purpose

A book probe replays a chapter mechanically. An adoption probe replays a
purpose: a pristine host, a one-page brief stating the purpose and its budget,
the book and nothing else, and a log in which every stop (a command that does
not exist, a word not defined, a wait longer than the budget, an error whose
remedy is not the next command) becomes an issue with a `purpose:` frontmatter
field (`industrial`, `field`, `education`).

The industrial and education probes can be run by Claude sessions given the
brief. The field probe is run by a real technician or a student, because its
budget is about what a person does with a box, and a model's reading of a
chapter is not evidence of that. Probes run in two rounds: once to find the
stops, once after the fixes to count what remains.

## Alternatives considered and rejected

- **Workspace-first, as today.** It is the right shape for a system of nodes,
  and the wrong first page for two of three purposes. Kept, moved to chapter
  three.
  (Since reversed: RFC-0121 made every project a workspace, so this
  alternative is now the design; D6 above says what holds.)
- **WiFi-first.** The board every field adopter wants has WiFi, and the
  driver nano-ros can reach on the pinned Zephyr 3.7 line does not drive it
  (the AIROC driver is SDIO-only there; the Pico W's part is on SPI/PIO and the
  GIGA R1's needs an STM32 SDHC host that 3.7 lacks). Deferred to 4.4 and said
  so, rather than promised and probed never.
- **ESP32 revival now.** The esp-hal road is dormant (issue 1525) and the
  Zephyr esp32 line is unsupported in the index. Reviving it costs a road.
  The picked boards cover field and education without it; the adoption probes
  decide whether the demand is real (see the decision steps below).
- **A per-purpose book.** Three books triple the probe cost and drift apart.
  One book with purpose columns in its acceptance is what the probes can keep
  honest.

## Evaluation and decision steps

Each decision is measured, and each has a point at which the measurement can
change it.

| Decision | Measured by | Decision point |
| --- | --- | --- |
| D1 budgets | the first chapter's probe prints its wall-clock time from `nros setup` to the asserted `probe-expect` line; the lab install probe prints install time; the offline probe passes or fails | a budget missed twice in a row on the probe is a phase work item, never a wider budget |
| D2 | `check-user-surface-ids` count, baseline zero after the sweep | a new id on the user surface fails the fast line; no allowlist |
| D5 | the serial chapter is probed on mps2-an385 QEMU (phase-525 W4) and on each emulator twin (phase-524); ethernet chapter two is probed on the Nucleo twin | if the serial link cannot be driven in an emulator twin, that board's chapter one opens over the emulator's console transport instead and says so, and D5 gains the exception |
| D6 | the field budget (30 minutes) measured by the probe on QEMU after phase-525 W3 | if the budget is missed on QEMU with the single package in place, D6 widens to a generated single-file project (one source file, no `system.toml` the reader edits) and phase-525 gains a work item |
| D7 | every chapter's run line equals `nros run` and the printed next step (the issue 2169 test extended to the verb) | a board whose run cannot be expressed by `nros run` is not a book board (RFC-0115 D4) |
| D9 | issue count per purpose, round one against round two; the field probe's recorded time | if round two for a purpose finds more stops than round one closed, that purpose's chapter is rewritten before more boards are added; if the field and education probes both ask for ESP32 by name, ESP32 revival opens as its own phase |

The RFC moves from Draft to Stable when phase-525 has landed W1 to W4 and the
round-two adoption probes have run for all three purposes.

## Relationship to the sibling RFCs

- RFC-0115 (emulator-first board policy) decides which boards the chapters
  cover and requires an emulator twin before a board enters the book (D3, D4,
  D8). This RFC's D5 and D7 assume those twins exist.
- RFC-0114 (board support registry) makes the registry the one primitive from
  which cells, probe tracks, run commands and the evidence page are generated
  (D10 to D12). `nros run` (D7) reads the registry row for its board; the
  evidence page that answers the industrial purpose's "what is measured" is
  that RFC's output.
