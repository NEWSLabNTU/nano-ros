# Phase 523 — The education lab series, the adoption probes and the industrial extras

**Status (2026-11-01). IN PROGRESS — W4 landed (offline probe track, a delta
page per fork, `nros doctor --report` with the toolchain); W3's briefs and the
`purpose:` field landed, its probe rounds not yet run; W1 and W2 wait on
phase-525 W3 (`nros run`) and phase-524 W4 (the Renode probe tracks).** Implements
[RFC-0116](../design/0116-adoption-purposes-and-front-doors.md) D1 and D9
and the industrial purpose's remaining frictions. Depends on
[phase-525](phase-525-adoption-front-doors.md) W3 (`nros run`, the grading
command) and on phase-524 W4 (the Renode and Wokwi probe tracks, so lab seven
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

- Labs 1 to 6 run on QEMU mps2-an385 and on the Renode twin of phase-524
  (the Nucleo-F767ZI; phase-524 W1 dropped the UNO R4 Minima, whose RA4M1 the
  C talker does not fit, and the Pico W, which Renode does not model), so a
  seat needs no hardware. Lab 7 runs the same image on the Nucleo bench, which
  the lab does not have yet: it is the one item that needs a board bought.
- Each lab is a chapter under `book/src/education/`, with a stated learning
  goal, the steps, the expected output as a `probe-expect` block, and the
  grading command `nros run --expect '<line>'` the instructor runs per seat.
- Proposed series: (1) first node over serial; (2) a listener and QoS;
  (3) a service; (4) parameters and lifecycle; (5) two tiers and timing (the
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
