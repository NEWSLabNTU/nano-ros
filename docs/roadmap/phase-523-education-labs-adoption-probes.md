# Phase 523 — The education lab series, the adoption probes and the industrial extras

**Status (2026-11-06). W1 moved to phase-537 W5 (maintainer). The release
behind 2255 is the maintainer's to cut; round two runs on the release after
it. Book-side round-one stops (2259, 2262, 2264–2268, 2272) are fixed after
phase-537's page moves (#3160) land; the CLI-side ones (2256–2258, 2260,
2261, 2263, 2270, 2271) are being fixed now. "Left" (2197) is open for a
comparison, not yet ruled.**

**Status (2026-11-05). W3 round one RUN for industrial and education:
industrial 18 stops → 14 open issues, education 14 stops → 12 open issues
(18 issues in all, 2255–2272; seven shared by both purposes). Field round one
not run (needs a person with a board). Both probes installed the newest
release, `nros-0.6.0-nros6`, which is 318 commits behind the `main` the book is
published from. Their first three stops are verbs that release lacks (issue
2255), so a release cut before round two is the cheapest fix of all. Reports:
[runs/](../development/adoption-probes/runs/).**

**Status (2026-11-01). IN PROGRESS — W4 landed (offline probe track, a delta
page per fork, `nros doctor --report` with the toolchain); W3's briefs and the
`purpose:` field landed, its probe rounds not yet run; W1 and W2 wait on
phase-528 W4 (`nros run`) and phase-524 W4 (the Renode probe tracks).** Implements
[RFC-0116](../design/0116-adoption-purposes-and-front-doors.md) D1 and D9
and the industrial purpose's remaining frictions. Depends on
[phase-528](phase-528-run-on-host-and-emulator.md) W4 (`nros run`, the grading
command; it carries phase-525 W3 since RFC-0118) and on phase-524 W4 (the Renode and Wokwi probe tracks, so lab seven
has a bench twin). Starts after both.

---

## Why

RFC-0116 D1 makes three purposes the acceptance columns of every user-facing
surface. Phase-525 serves the shared frictions on QEMU; phase-524 gives the
boards their twins. What remains is purpose-specific: the education purpose
needs a lab series, a lab image and a semester pin recipe; the industrial
purpose needs the offline install, the fork deltas and a conformance report;
and all three need the method that finds the next friction before an adopter
does — the adoption probes of D9, run twice.

## Work items

### W1 — Six labs on QEMU and Renode, lab seven on the bench

**Moved to [phase-537](phase-537-book-user-guide-organization.md) W5
(maintainer, 2026-11-06).** That phase's Tutorial is this lab series: ten
chapters, each a two-hour lab with a probe track, accepted by an education
adoption probe. Its "Absorbed from phase-523 W1" section carries what is
below and not in its chapter list: the QoS, lifecycle and two-tier timing labs,
the Renode twin run, lab 7 on the bench, and the grading command per chapter.
The text below is kept as the record of what was asked.

- Labs 1 to 6 run on QEMU mps2-an385 and on the Renode twin of phase-524
  (the Nucleo-F767ZI; phase-524 W1 dropped the UNO R4 Minima, whose RA4M1 the
  C talker does not fit, and the Pico W, which Renode does not model), so a
  seat needs no hardware. Lab 7 runs the same image on the Nucleo bench, which
  the lab does not have yet: it is the one item that needs a board bought.
- Each lab is a chapter under `book/src/education/`, with a stated learning
  goal, the steps, the expected output as a `probe-expect` block, and the
  grading command `nros run --expect '<line>'` the instructor runs per seat.
- Proposed series: (1) first node over serial; (2) a listener and QoS;
  (3) a service; (4) parameters and lifecycle; (5) two sched tasks and timing (the
  real-time promise, measured); (6) talking to a ROS 2 graph on the host;
  (7) the same image flashed to the Nucleo and run over ethernet.
- Every lab is probe-tagged and joins `probe-platforms.yml`, generated from the
  registry once phase-522 lands.

Acceptance: `just probe <lab>` passes for all seven, lab 7 in manual cadence
with a dated run; each lab's grading command is the chapter's own line.

Decision step: a lab whose expected output is not deterministic across seats
(a timing number, a discovery order) states a range or a predicate the grading
command can check; a lab that cannot be made deterministic is dropped from
the graded series and kept as a demonstration.

### W2 — The instructor image and the semester pin

- A container image with the toolchain, the Zephyr workspace with the module
  allowlist, the release `nros` and the lab sources pre-fetched, built by a
  lane and published with the release. Install on a seat is a pull.
- A semester pin recipe in lab 0: `nros pin` to the release, the lock
  committed in the course repository, and what to do when a student's seat
  disagrees.
- The install probe times a fresh seat from pull to lab 1's first output and
  fails over 15 minutes; a seat behind a proxy is probed with the container's
  proxy variables set.

Acceptance: the image builds on the release lane; the install probe prints a
time under the budget on the probe runner; lab 0 is probe-tagged.

Decision step: if the image exceeds the size a lab network can pull in the
budget (measured on the probe runner with a throttled link), the Zephyr
workspace moves out of the image into a mirrored fetch the lab host serves.

### W3 — Adoption probes, two rounds

- A one-page brief per purpose: the purpose, its budget, the host it starts
  on, the book as the only source. The industrial and education probes are
  run by Claude sessions given the brief; the field probe is run by a real
  technician or a student with a board and a laptop.
- Every stop becomes an issue with `purpose:` in its frontmatter; the issue
  index groups by it.
- Round one before the fixes it finds; round two after. The counts per
  purpose are recorded in this phase's status line.

**Landed (2026-11-01).** The briefs and the method are in
[docs/development/adoption-probes/](../development/adoption-probes/README.md):
one each for industrial and education (run by a Claude session that has read
nothing but the brief and the book) and one for field (run by a person with a
board). A stop is filed with `purpose:` in its frontmatter; `just issues
--purpose <p>` lists one purpose's, and the generated `open.md` groups them.

**Round one (2026-11-05), industrial and education.** Two fresh Claude sessions
ran the briefs. Each had only the brief, the stop definition and the book
(markdown from `main` at `03473878d6`), in its own `ubuntu:24.04` container,
and installed the newest release `nros-0.6.0-nros6`.

| Purpose | Stops | Open issues | Tasks |
| --- | --- | --- | --- |
| industrial | 18 | 14 | 1–3 done with stops (first `ros2 topic echo` at +18.9 min; byte-identical image at one path; offline to the boot banner); 4 partly possible; 5 not possible from the book |
| education | 14 | 12 | install 12 min 04 s to the promised line, but only with four off-book workarounds (the book alone stops at 7:05 on `nros run`); 1–4 done with stops; first error does not name the next command |

A stop both probes hit is one issue listing both purposes
(`purpose: [industrial, education]`; `scripts/lib/issue_purpose.py` reads it
for `just issues` and `open.md`). One industrial stop was not refiled: offline
cargo is fixed on `main` by issue 2197 and missing only from the release.
Every stop was checked against `main` before filing.

Issues homed here (each has a `**Home:**` line pointing here):

| issue | purpose | what |
| --- | --- | --- |
| [#2255](../issues/2255-book-ahead-of-latest-release.md) | both | The book teaches `nros dev`, `nros run`, `nros new --board`; the latest release has none of them |
| [#2256](../issues/2256-store-label-disagrees-with-provenance.md) | both | A store source dir is labelled with one pin and holds another; the build's zenoh-pico refusal loops |
| [#2257](../issues/2257-setup-and-build-read-different-index.md) | both | `nros setup` provisions from `main`'s index, while build and locate read the release's; the QEMU remedy loops |
| [#2258](../issues/2258-setup-rewrites-pinned-sdk-lock.md) | both | `nros setup` rewrites a committed `nros-sdk.lock`, so a semester pin does not hold |
| [#2259](../issues/2259-first-project-requires-humble-on-noble.md) | both | First Project requires Humble, which Ubuntu 24.04 cannot install |
| [#2260](../issues/archived/2260-ros-edition-mismatch-silent-no-delivery.md) | both | **Resolved** 2026-11-06: the scaffold follows `$ROS_DISTRO`, and `nros run` warns on a mismatch |
| [#2261](../issues/2261-catkin-pkg-prereq-breaks-ros-apt.md) | both | The `python3-catkin-pkg` prerequisite breaks the ROS 2 apt install |
| [#2262](../issues/2262-serial-quickstart-router-unreachable-from-ros2.md) | industrial | The serial quick-start router listens on serial only |
| [#2263](../issues/archived/2263-setup-rejects-board-key-nros-new-writes.md) | both | **Resolved** 2026-11-06: fixed on `main` by phase-531 W4's scaffold (`b719d169ed`); only the release lacked it |
| [#2264](../issues/2264-airgap-recipe-omits-host-packages-and-asset-names.md) | industrial | The air-gapped recipe omits apt packages and release asset names |
| [#2265](../issues/2265-memory-evidence-needs-source-tree.md) | industrial | Memory evidence needs a checkout; the documented heap is wrong |
| [#2266](../issues/2266-book-has-no-third-party-inventory.md) | industrial | No third-party or patch inventory in the book |
| [#2267](../issues/2267-book-has-no-release-provenance-report.md) | industrial | No release provenance report reachable from the book |
| [#2268](../issues/2268-no-release-support-backport-policy.md) | industrial | No release support or backport policy; stale Jazzy line |
| [#2269](../issues/2269-no-emulator-chapters-for-lab-topics.md) | education | Five of seven lab topics have no emulator chapter. Home moved to phase-537 W5 with W1 (2026-11-06) |
| [#2270](../issues/2270-default-talker-prints-internal-warnings.md) | education | The default talker prints internal issue ids and a "contract violation" |
| [#2271](../issues/archived/2271-install-sh-no-longer-prints-path-export.md) | education | **Resolved** 2026-11-06: not reproduced — install.sh prints the line, above the closing `next` lines |
| [#2272](../issues/2272-serial-flow-needs-socat-unlisted.md) | education | The serial flow needs `socat`, which no prerequisite list names |
Round one has not run.

Acceptance: three briefs committed under `docs/development/adoption-probes/`;
round-one and round-two issue counts per purpose recorded; round two's field
run is under the 30-minute budget on a real board.

Decision step: if round two for a purpose finds more stops than round one
closed, that purpose's chapters are rewritten before phase-524 adds another
board. If the field and education probes both ask for ESP32 by name, ESP32
revival opens as its own phase (RFC-0116's evaluation table).

### W4 — The industrial extras

- An offline install probe: the release asset and the SDK store are staged on
  a host with no network, and the first chapter runs to its asserted line.
- A fork delta page for every fork the tree carries (today only cyclonedds has
  one), generated from the fork's branch against its upstream pin.
- `nros doctor --report`: the resolved toolchain, store, pins, the board's
  registry row and its evidence, as one document an auditor can file.

Acceptance: the offline probe is a track; `check-doc-refs` finds a delta page
for every fork in the index; `--report` output is reproducible across two runs
on the same host.

Decision step: a fork whose delta cannot be stated as a patch list against a
pinned upstream is a fork whose pin is wrong, and is filed as such rather than
documented as is.

**Landed (2026-11-01).**

- *Offline probe.* `just probe offline` (track `offline` in the board
  registry, weekly) replays the bare-metal chapter twice: online to stage the
  release asset and the SDK store, then on a commit of that container with
  `--network none` and `NROS_OFFLINE=1`, from the staged store and nothing
  else. It follows the book's own contract, `vendored-tree.md` "Air-gapped /
  mirrored CI". Its first runs found two stops in that contract, both fixed
  ([issue 2197](../issues/archived/2197-offline-contract-omits-rust-homes.md)):
  rustup keeps the Rust toolchain outside `NROS_HOME`, and `NROS_OFFLINE` did
  not reach cargo (it now sets `CARGO_NET_OFFLINE`). Measured passing
  2026-11-01.
- *Fork deltas.* `docs/reference/forks/` holds one generated page per fork
  (nine: zenoh-pico, cyclonedds, lwip, threadx, netxduo, nuttx, libc, qemu,
  ros2-rust-examples), each the commit list from GitHub's compare API between
  the pinned fork commit and a named upstream ref. Seven pins descend from an
  upstream release tag; libc is measured against upstream's `libc-0.2` branch
  and the examples against `main`, each at its recorded merge base. No fork
  failed the decision step. `scripts/gen-fork-deltas.py` writes them (needs
  `gh`); `just check fork-deltas` (offline, fast line) refuses a branch-tracking
  submodule with no row and a page whose pin is not the superproject's.
- *`nros doctor --report`.* phase-522 W5 gave it each image's board row and
  status; this adds the store, the index, the pin files, and per image the
  toolchain its board and RMW provision (each package's pin, store prefix and
  what `nros setup` would do), resolved through the same plan `nros setup`
  runs. Two runs on one host print the same text.

## Acceptance

| Work item | Industrial | Field solutions | Education |
| --- | --- | --- | --- |
| W1 | lab 5 is a measured real-time demonstration | labs 1 to 3 are the technician's own first week | seven graded labs, six without hardware |
| W2 | — | — | 15-minute install measured; semester pin taught |
| W3 | round-two stop count, industrial | round-two time under 30 minutes on a real board | round-two stop count, education |
| W4 | offline install passes; every fork has a delta page; `doctor --report` | — | — |

## Left

- Whether `nros setup` should install the Rust toolchain under `NROS_HOME`
  (`RUSTUP_HOME`/`CARGO_HOME` set for the rustup it runs), so the offline
  contract is one directory again. It moves an existing user's toolchain, so it
  needs an RFC-0116 decision first (issue 2197).

## Out of scope

Hardware runners (lab 7 stays manual cadence), Windows or macOS native
toolchains, ESP32 until W3 reopens it, a per-purpose book.

## RFC-0120 (2026-11-03): what changes for this phase

[RFC-0120](../design/0120-nros-configure-one-resolver.md) adds `nros configure`. It covers what `nros sync` did, plus census, resolve and generated files. Every road calls it, and `nros build`/`nros run` call it implicitly when stale, like cargo. `nros sync` becomes its alias. A leaf that states no entities is sized from its census, and a host C/C++ compiler is required. Every host configure takes one pass, including a plain `west build`. The work is [phase-530](phase-530-nros-configure-single-pass.md). For this phase:

- **W1, W3.** Lab and probe command sequences have no `nros sync` step. `nros build` / `nros run` configure implicitly, so a probe's timed path includes configure.
- **W2, the instructor image.** It must carry a host C/C++ compiler: the census sizes every undeclared leaf, the Zephyr hardware labs included (RFC-0120 D4).
