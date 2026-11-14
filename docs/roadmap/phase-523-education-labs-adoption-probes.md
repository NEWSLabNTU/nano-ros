# Phase 523 — The education lab series, the adoption probes and the industrial extras

**Status (2026-11-07, round two). W3 round two RUN for industrial and
education on `nros-0.6.0-nros7`: industrial 13 stops, education 8 stops →
17 issues (2317, 2319–2334; four stops shared). Both are below what round one
closed (industrial 14, education 11), so the decision step rewrites no
chapter. One stop was a regression from a round-one fix (2317: the release's
First Project did not compile), now fixed on `main`; one (the crate listing
needing `jq`) was fixed while filing. Neither probe asked for ESP32. Left: the
field probe, and the 16 open round-two issues.**

**Status (2026-11-07, later). Release `nros-0.6.0-nros7` is cut from `main`
(538ccb9084) and pinned in the index, so 2255 is closed and W3 round two runs
against it next. Every round-one stop is closed but 2269 (homed in phase-537). Fixed on `main`:
2256–2268, 2270–2272 and 2197; 2262 and 2272 were already fixed by
phase-537's serial page. Release support is ruled: latest release only. One
issue filed from the work, 2309, was retracted: it was measured on a stale image, and the heap is in `.bss`.
Left: round two on the next release, the field probe (a person with a board),
and W2 (phase-537 W5's lab sources).**

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
[phase-528](archived/phase-528-run-on-host-and-emulator.md) W4 (`nros run`, the grading
command; it carries phase-525 W3 since RFC-0118) and on phase-524 W4 (the Renode probe track; the Wokwi one was removed by the 2026-11-06 ruling, RFC-0115 Amendment 1; so lab seven
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
- Every lab is probe-tagged and joins `weekly.yml`, generated from the
  registry once phase-522 lands.

Acceptance: `just probe <lab>` passes for all seven, lab 7 in manual cadence
with a dated run; each lab's grading command is the chapter's own line.

Decision step: a lab whose expected output is not deterministic across seats
(a timing number, a discovery order) states a range or a predicate the grading
command can check; a lab that cannot be made deterministic is dropped from
the graded series and kept as a demonstration.

### W2 — The instructor image and the semester pin

**Superseded (2026-11-11) by [phase-551](phase-551-starter-image.md)** and
[RFC-0127](../design/0127-starter-image.md): the user widened the instructor
image to a general starter image (newcomers, educators, customers), of which
the course seat is one recipe (RFC-0127 D4). The text and open questions below
are kept as the record they grew from; nothing here is built.

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

**Open questions for the user (2026-11-11), before any image is built.** W2
was written when the labs were planned on Zephyr. They have since landed on
`mps2-an385-freertos` and `mps2-an385-baremetal`: the Tutorial's C and C++
tabs (issue 2376), the QoS lab, and the two-tier lab. So what the image must
carry has changed. Sizes measured on a provisioned host, uncompressed:

| Piece | Size | Needed by |
|---|---|---|
| `ros:humble-ros-base` (22.04) / `ros:jazzy-ros-base` (24.04) base | 758 MB / 888 MB | the router (`rmw_zenohd`), `ros2` in chapter 10; plus `rmw-zenoh-cpp` and `example-interfaces` |
| arm-none-eabi-gcc 13.2 (store) | 1.2 GB | every MPS2 image, all languages |
| QEMU 11.0 (store) | 486 MB | every lab run |
| FreeRTOS kernel, lwIP, zenoh-pico, mbedTLS (store sources) | 77 MB | FreeRTOS images; zenoh-pico and mbedTLS for bare metal too |
| Rust (rustup, not the store, ruled 2026-11-06): host std 370 MB, `thumbv7m-none-eabi` std 65 MB, plus rustc/cargo and `rust-src` | ~1 GB in a minimal profile (not measured; the host's own install is 3.1 GB with every target) | the Rust tabs |
| Zephyr SDK 0.16.8 (all arches) / Zephyr workspace (allowlisted modules) | 7.8 GB / 1.7 GB | no current lab; the Zephyr chapters of the book |

The decisions that only the user can make:

1. **Hosting.** Which registry, and who may pull: `ghcr.io/newslabntu/…`
   public, or a private or university mirror. Also the image name and its tag
   scheme (one tag per release, `nros-<version>`, or per semester).
2. **What ships.**
   - **Boards.** FreeRTOS and bare metal only, about 4 GB uncompressed with
     ROS and Rust, or Zephyr too, about +9.5 GB. That is the W2 text's
     "Zephyr workspace", which no current lab uses.
   - **ROS 2 edition.** Humble on 22.04, which every probe track uses today,
     or Jazzy on 24.04.
   - **Rust.** Include it, or offer a C/C++-only variant without it.
3. **"Lab sources pre-fetched".** The labs now scaffold with `nros new`, so
   there are no lab sources to fetch. Should the image instead pre-fetch the
   crates the Rust tabs pull from crates.io, so a seat behind a NAT never
   reaches it? Or is the course repository of the semester pin a separate
   repository the image should clone? If so, which one, and who owns it?
4. **Build cadence.** On every release in `release-nros.yml` (the W2 text),
   which adds the image build to each release, or on dispatch once per
   semester.
5. **The pull budget's link.** The decision step measures "a throttled
   link". What bandwidth stands for a lab network: 100 Mbit/s shared by 30
   seats, or 1 Gbit/s? At 4 GB, 15 minutes needs ~36 Mbit/s per seat.

Until these are answered, W2 builds nothing.

### W3 — Adoption probes, two rounds

- **Status:** DONE (2026-11-01)

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
| [#2255](../issues/archived/2255-book-ahead-of-latest-release.md) | both | **Resolved** 2026-11-07: release `nros-0.6.0-nros7` cut from main; every measured verb present |
| [#2256](../issues/archived/2256-store-label-disagrees-with-provenance.md) | both | **Resolved** 2026-11-06: one index per release, so the store label and its contents share a pin |
| [#2257](../issues/archived/2257-setup-and-build-read-different-index.md) | both | **Resolved** 2026-11-06: a release reads its own shipped index (RFC-0097 D5 amended), so setup and build agree |
| [#2258](../issues/archived/2258-setup-rewrites-pinned-sdk-lock.md) | both | **Resolved** 2026-11-06: with one index per release, a pinned seat's setup records the lock's own pins |
| [#2259](../issues/archived/2259-first-project-requires-humble-on-noble.md) | both | **Resolved** 2026-11-07: Book: the ROS 2 edition follows the Ubuntu release |
| [#2260](../issues/archived/2260-ros-edition-mismatch-silent-no-delivery.md) | both | **Resolved** 2026-11-06: the scaffold follows `$ROS_DISTRO`, and `nros run` warns on a mismatch |
| [#2261](../issues/archived/2261-catkin-pkg-prereq-breaks-ros-apt.md) | both | **Resolved** 2026-11-07: Book: install ROS 2 before the host packages |
| [#2262](../issues/archived/2262-serial-quickstart-router-unreachable-from-ros2.md) | industrial | **Resolved** 2026-11-07: Fixed on main: the serial router also listens on TCP |
| [#2263](../issues/archived/2263-setup-rejects-board-key-nros-new-writes.md) | both | **Resolved** 2026-11-06: fixed on `main` by phase-531 W4's scaffold (`b719d169ed`); only the release lacked it |
| [#2264](../issues/archived/2264-airgap-recipe-omits-host-packages-and-asset-names.md) | industrial | **Resolved** 2026-11-07: Book: the offline recipe carries apt packages and names the asset |
| [#2265](../issues/archived/2265-memory-evidence-needs-source-tree.md) | industrial | **Resolved** 2026-11-07: Book: measure memory without a checkout; the heap and stack figures |
| [#2266](../issues/archived/2266-book-has-no-third-party-inventory.md) | industrial | **Resolved** 2026-11-07: Book: a third-party inventory and the fork pages |
| [#2267](../issues/archived/2267-book-has-no-release-provenance-report.md) | industrial | **Resolved** 2026-11-07: Book: name `nros doctor --report` and date support per release |
| [#2268](../issues/archived/2268-no-release-support-backport-policy.md) | industrial | **Resolved** 2026-11-07: Book: release support policy (latest only) |
| [#2269](../issues/2269-no-emulator-chapters-for-lab-topics.md) | education | Five of seven lab topics have no emulator chapter. Home moved to phase-537 W5 with W1 (2026-11-06) |
| [#2270](../issues/archived/2270-default-talker-prints-internal-warnings.md) | education | **Resolved** 2026-11-06: the descriptor writer reports defaults as one line, the scaffold declares its contract, and no user-facing text cites a tracker id (gated) |
| [#2271](../issues/archived/2271-install-sh-no-longer-prints-path-export.md) | education | **Resolved** 2026-11-06: not reproduced — install.sh prints the line, above the closing `next` lines |
| [#2272](../issues/archived/2272-serial-flow-needs-socat-unlisted.md) | education | **Resolved** 2026-11-07: Fixed on main: no `socat` in the serial flow |
| [#2309](../issues/archived/2309-heap-static-lands-in-data.md) | both | **Retracted** 2026-11-07: measured on a stale image; a fresh build places HEAP in `.bss` |
Round one has not run.

Acceptance: three briefs committed under `docs/development/adoption-probes/`;
round-one and round-two issue counts per purpose recorded; round two's field
run is under the 30-minute budget on a real board.

Decision step: if round two for a purpose finds more stops than round one
closed, that purpose's chapters are rewritten before phase-524 adds another
board. If the field and education probes both ask for ESP32 by name, ESP32
revival opens as its own phase (RFC-0116's evaluation table).

**Round two (2026-11-07), industrial and education.** Two fresh Claude
sessions, one after the other, ran the same briefs against the book exported
from `main` and the release `nros-0.6.0-nros7` installed by `install.sh` in
pristine `ubuntu:24.04` containers with Jazzy. Reports:
[runs/](../development/adoption-probes/runs/) (`2026-11-07-round-2-*`).

| Purpose | Stops | Round one closed | Rewrite? | Headline |
| --- | --- | --- | --- | --- |
| industrial | 13 | 14 | no | First Project compiles only with a knob edit (2317); Jazzy echo receives nothing (2320); provenance and reproducibility gaps (2321, 2322, 2327, 2328) |
| education | 8 | 11 | no | install to first QEMU output 8 min 55 s (budget 15); same compile stop and echo stop; build output vocabulary (2332) |

| issue | purpose | what |
| --- | --- | --- |
| [#2317](../issues/archived/2317-release-sidecars-always-stale-talker-max-cbs-zero.md) | both | **Resolved** 2026-11-07: release sidecars always read stale, and the talker contract sized its callback table to 0 |
| [#2319](../issues/archived/2319-first-project-scaffold-before-ros-source.md) | both | **Resolved** 2026-11-07: Tutorial 1 and every starter source ROS 2 before they scaffold |
| [#2320](../issues/archived/2320-jazzy-echo-qemu-talker-type-hash-invalid.md) | both | **Resolved** 2026-11-07: workspace sync generated Humble messages; it now takes the bringups' edition, and Jazzy echo receives the talker |
| [#2321](../issues/archived/2321-lock-provenance-incomplete.md) | industrial | **Resolved** 2026-11-07: source builds lock their origin; `[rust]` recorded |
| [#2322](../issues/2322-image-not-reproducible-across-paths.md) | industrial | Open: a rebuild at another path differs because cargo hashes the absolute path of local crates; the book states the contract (same path ⇒ same bytes) |
| [#2385](../issues/archived/2385-ws-link-breaks-generated-crate-paths-that-climb-out.md) | industrial | **Resolved** 2026-11-10: #3352 reverted; #2322 reopens and its redo keeps every routed crate's relative paths inside the workspace |
| [#2323](../issues/archived/2323-offline-deb-recipe-omits-ros-base.md) | industrial | **Resolved** 2026-11-07: the recipe downloads in plain Ubuntu with the ROS 2 source; 606 debs, 227 MB |
| [#2324](../issues/archived/2324-offline-error-no-nros-offline-hint.md) | industrial | **Resolved** 2026-11-07: a name-resolution failure in the metadata probe names `NROS_OFFLINE=1` |
| [#2325](../issues/archived/2325-board-support-no-last-run-date.md) | industrial | **Resolved** 2026-11-07: the release records each lane's last run; `board show` and doctor print it |
| [#2326](../issues/archived/2326-mem-report-not-shipped.md) | industrial | **Resolved** 2026-11-07: the report runs from a release; setup installs llvm-tools |
| [#2327](../issues/archived/2327-per-image-c-components-unlisted.md) | industrial | **Resolved** 2026-11-09: doctor lists the sources each build read; the forks stay on the maintainer's account, and the forks index says so |
| [#2328](../issues/archived/2328-doctor-report-missing-release-facts.md) | industrial | **Resolved** 2026-11-07: doctor prints Rust, ROS 2, digests, sources and the image digest |
| [#2329](../issues/archived/2329-no-lts-security-eol-policy.md) | industrial | **Resolved** 2026-11-07: SECURITY.md, a CycloneDX SBOM per release, the policy in the book |
| [#2330](../issues/archived/2330-ros-install-leaves-book.md) | education | **Resolved** 2026-11-07: Install adds the ROS 2 apt source inline, at a pinned version |
| [#2331](../issues/archived/2331-book-links-repo-and-rfcs.md) | education | **Resolved** 2026-11-07: RFC numbers stay as plain text in "Design background"; no repository links |
| [#2332](../issues/archived/2332-build-output-internal-vocabulary.md) | education | **Resolved** 2026-11-07: build detail moved behind `-v`, the talker scaffold declares its external reader, and the MAX_CBS refusal names the `system.toml` knob |
| [#2333](../issues/archived/2333-sdk-lock-lands-in-cwd.md) | both | **Resolved** 2026-11-07: setup outside a project locks in the store, and Install explains the lock |
| [#2334](../issues/archived/2334-nros-version-hides-release-suffix.md) | both | **Resolved** 2026-11-07: `nros version` and `doctor --report` print the release version with its suffix |

Found after round two, while fixing the probes:

| issue | purpose | what |
| --- | --- | --- |
| [#2346](../issues/archived/2346-quickstart-probe-no-ros-vs-configure-refusal.md) | both | **Resolved** 2026-11-07: the checkout probe's quickstart track runs `ros:jazzy` and sources it; `activate.sh` keeps a sourced ROS 2 |
| [#2348](../issues/archived/2348-rust-workspace-listener-arena-underderived.md) | both | **Resolved** 2026-11-07: the Rust probe states the buffered row beside `in_place`; the observation outranks the backend default |

### W4 — The industrial extras

- **Status:** DONE (2026-11-01)

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
- *Fork deltas.* `book/src/reference/forks/` (moved into the user book by issue 2266, with internal ids taken out of the commit subjects; the table stays at `docs/reference/forks/forks.toml`) holds one generated page per fork
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

### W5 — No internal ids on the user surface (issues 2305, 2443)

**User ruling, 2026-11-12** ("Take your recommendation", option A): user-reachable
output, build-stopping refusals included, names what, where and how to fix,
and no phase, issue or RFC id. Ids live in source comments (`// History:`,
`# History:`) and `-v` detail. Stable error codes (ruling C) come later, as
phase-552; each message stays self-contained so a code can go in front.

The backlog starts as counted ratchets that may only fall:
`check-user-surface-ids` (CLI library string literals and build-script panic
texts, `.config/user-surface-ids-ratchet.json`) and
`check-cmake-user-output-refs` (now counting `FATAL_ERROR`/`SEND_ERROR` too,
`.config/cmake-user-output-refs.json`). The sweep goes by file, biggest first;
each batch lowers its baseline in the same PR. When a ratchet reaches zero it
becomes a hard rule in the next change: its baseline file goes, and any id
fails with its file and line (the Rust half did so after batch 10, the
CMake half after CMake batch 2).

| Step | PR | Rust ratchet | CMake ratchet | Status |
| --- | --- | --- | --- | --- |
| colcon plugin, `build_type.rs`, `package_xml.rs`, `NanoRosPackageXml.cmake` | #3593 | (gated at zero) | — | merged |
| ratchets added | #3617 | 228 | 102 | merged |
| batch 1: `entity_inventory.rs`, `cmake_root.rs`, `west_app.rs` | #3621 | 178 | 102 | merged |
| batch 2: `nros_image_kconfig.cmake`, `NanoRosNodeRegister.cmake`, `nros_cargo_build.cmake`, `nros_rmw_zenoh.cmake` | #3628 | 178 | 80 | merged |
| batch 3: `cargo_metadata_schema.rs` (manifest and `system.toml` refusals, the WCET host-profile refusal) | #3666 | 168 | 80 | merged |
| batch 4: `leaf_entity_env.rs`, `facade.rs`, `sizing_descriptor.rs` (generated-file headers, the sizing refusals) | #3698 | 139 | 80 | merged |
| batch 5: `resolve.rs`, `metadata_build.rs`, `scaffold.rs` (the resolve record, the metadata-probe harness, `nros new` output) | #3734 | 115 | 80 | merged |
| batch 6: `nros_config.rs`, `metadata_probe_cmake.rs`, `message_bound_knobs.rs` (+ its CMake twin), `builder/dist.rs` (the retired-`nros.toml` and node-alias refusals, the metadata-probe CMakeLists, the derived knobs file, the image manifest and its refusals) | #3740 | 88 | 80 | merged |
| batch 7: `nros-rmw-xrce-cffi/build_symbol_prefix.rs`, `cargo-nano-ros/build.rs`, `entity_census.rs` (the XRCE symbol-prefix build checks and generated header, the backend/serdes table refusals and generated headers, the census verdict notes) | #3750 | 75 | 80 | merged |
| batch 8: `nros-node/build.rs` and the orchestration `plan.rs`, `image_net.rs`, `rmw_config.rs`, `board_descriptor.rs`, `model_ingest.rs`, `release_manifest.rs`, `backend_link.rs` (the sizing refusals, the retired-key remedies, the resolved-network and release-manifest headers, the RMW-file and backend-link refusals) | #3773 | 47 | 80 | merged |
| batch 9: the `cargo-nano-ros` `extension_descriptor.rs`, `rmw_descriptor.rs`, `serdes_descriptor.rs`, `serdes_resolver.rs`, `lib.rs` and the builder `cargo_config.rs`, `cargo_root.rs`, `discover.rs`, `entry.rs`, `handoff.rs`, `materialize.rs`, `paths.rs`, `plan.rs` (the extension, RMW and serdes descriptor refusals, the C++ alias header, the generated cargo config and entry headers, the workspace-root, duplicate-package, execvp and ESP32 refusals) | #3780 | 27 | 80 | merged |
| batch 10: the codegen `builtin_verbs.rs`, `entry/lower.rs`, `entry/mod.rs`, `entry/priority_record.rs`, `entry/px4.rs`, `contract_join.rs`, `declared_params_header.rs`, `queue_depth.rs`, `routing.rs`, `stale_guard.rs`, the orchestration `board_projection.rs`, `drift.rs`, `metadata_refresh.rs`, `planner.rs`, `registry_pins.rs`, `release_packages.rs`, `sdk_index.rs`, `workspace.rs`, and the `nros-params`, `cyclonedds-sys`, `nros-rmw-xrce-cffi` build scripts (the built-in verb list, priority-plan, declared-params, board cmake-vars and registry pin-set headers, the sched-task, PX4 and contract-join refusals, the drift, deprecation, build-type and stale-CLI messages, the build-script panics) | — | 0 | 80 | open |
| Rust half a hard rule: `check-user-surface-ids` drops `.config/user-surface-ids-ratchet.json`; any id in a CLI-library string or build-script panic text fails with its file and line | #3784 | rule | 80 | merged |
| CMake batch 1: every module under `cmake/` (`NanoRos*.cmake`, `board/`, `find/`, `toolchain/`: the configure refusals for retired names and keywords, domain, edition and deploy disagreements, missing tools, inventories and generated files, the SystemModel, promise and codegen-version refusals, the cross-toolchain change) | #3786 | 0 | 17 | merged |
| CMake batch 2: the PX4 (`NanoRosArchivePairing.cmake`, `NanoRosPx4Module.cmake`), NuttX (`nros-nuttx.cmake`, `nros-nuttx-depfile.cmake`), Cyclone DDS (`NrosRmwCycloneddsDescriptorCap.cmake`, `NrosRmwCycloneddsTypeSupport.cmake`) and Zephyr (`nros_checkout_guard.cmake`, `nros_cyclonedds_typesupport.cmake`, `nros_system_generate.cmake`, `nros_system_image_agreement.cmake`) modules and `scripts/check/rtos-feature-set-excludes-analysis.cmake` (the header/archive pairing and stray-backend refusals, the missing NuttX tree and dep-info, the descriptor-cap and idlc refusals, the two-checkout and foreign-SDK refusals, the baked-domain, bringup and image-agreement refusals) | #3787 | 0 | 0 | open |
| CMake half a hard rule: `check-cmake-user-output-refs` drops `.config/cmake-user-output-refs.json` and `--update`; any id in a default-level `message()` or `COMMENT` fails with its file and line | #3787 | rule | rule | open |
| runtime logs and panic text of a default image (C lexer, log/panic rule; 101 ids in 33 files fixed) | #3789 | (gated at zero) | — | merged |
| shell-script output (user-run scripts) | #3790 | (gated at zero) | — | merged |
| compile-time text (`#error`, `static_assert`, `compile_error!`, `#[deprecated]`, proc-macro errors): its own subject, baseline zero | — | — | — | planned |
| text assembled away from its print (`Display` impls, messages logged later): its own subject, baseline zero | — | — | — | planned |

Acceptance: both ratchet baselines reach zero and their subjects become
baseline-zero rules; runtime logs of a default image are read by a gate at
zero; issues 2305 and 2443 are archived.

## Acceptance

| Work item | Industrial | Field solutions | Education |
| --- | --- | --- | --- |
| W1 | lab 5 is a measured real-time demonstration | labs 1 to 3 are the technician's own first week | seven graded labs, six without hardware |
| W2 | — | — | superseded by phase-551 (the starter image) |
| W3 | round-two stop count, industrial | round-two time under 30 minutes on a real board | round-two stop count, education |
| W4 | offline install passes; every fork has a delta page; `doctor --report` | — | — |
| W5 | no internal id in a refusal or a generated file | no internal id in a refusal | no internal id in a refusal |

## Left

- ~~Whether `nros setup` should install the Rust toolchain under `NROS_HOME`.~~
  **Ruled 2026-11-06 (maintainer): no.** Rust stays in rustup's own homes.
  Owning it would mean a second toolchain per channel (1–2 GB each), a shell
  `cargo` and rust-analyzer that differ from nros's, and `RUSTUP_HOME`/
  `CARGO_HOME` carried at every place nros starts cargo, cmake or west. Moving
  `CARGO_HOME` also re-fetches the registry and moves cache keys, issue
  0491's class. The offline contract is therefore three directories, not
  one: `nros setup --offline-dirs` prints them (the store, then rustup's and
  cargo's homes) for `tar`. The book's air-gapped recipe is to name them
  (issue 2264, after phase-537's page moves).

## Book probe walks

Mechanical replays of book chapters with `just probe <track>`, outside the
scheduled lanes. A walk is not an adoption probe: it checks that the commands
print what the chapter says, not that a reader gets through.

**2026-11-12, the Zephyr chapters on the installed road.** Main `7afd109a6e`,
Zephyr 3.7.0 and SDK 0.16.8 (the pin before the 3.7.2 move). One release
asset built with `just probe platform-asset`, then
`PROBE_ASSET=<asset> just probe <track>` on host 139. Every track installs the
release with no nano-ros checkout on the host.

| Track | Chapter | Verdict | Wall time |
| --- | --- | --- | --- |
| `zephyr` | Zephyr native_sim, C | pass | 46 min |
| `zephyr-cpp` | Zephyr native_sim, C++ | pass | 46 min |
| `zephyr-rust` | Zephyr native_sim, Rust | pass | 50 min |
| `zephyr-west-app` | An existing west application | pass | 34 min |
| `zephyr-mps2` | mps2-an385-zephyr, Rust talker | **fail** | 32 min |

The mps2 failure is not specific to the Zephyr version. The image links, then
the stack-floor check (`scripts/nros-stack-floor.py`, `load_toml`) imports
`tomllib`, which needs Python 3.11, and falls back to `tomli`, which the host
does not have. On Ubuntu 22.04's Python 3.10 the build stops with
`ModuleNotFoundError: No module named 'tomli'`. native_sim does not run that
check, so its tracks pass. Archived issue 1364 fixed the same class for the CI
image only. The fix is routed to the stack-floor owner. Re-run `zephyr-mps2`
once it lands, and every track once the 3.7.2 pin lands.

## Out of scope

Hardware runners (lab 7 stays manual cadence), Windows or macOS native
toolchains, ESP32 until W3 reopens it, a per-purpose book.

## RFC-0120 (2026-11-03): what changes for this phase

[RFC-0120](../design/0120-nros-configure-one-resolver.md) adds `nros configure`. It covers what `nros sync` did, plus census, resolve and generated files. Every road calls it, and `nros build`/`nros run` call it implicitly when stale, like cargo. `nros sync` becomes its alias. A leaf that states no entities is sized from its census, and a host C/C++ compiler is required. Every host configure takes one pass, including a plain `west build`. The work is [phase-530](phase-530-nros-configure-single-pass.md). For this phase:

- **W1, W3.** Lab and probe command sequences have no `nros sync` step. `nros build` / `nros run` configure implicitly, so a probe's timed path includes configure.
- **W2, the instructor image.** It must carry a host C/C++ compiler: the census sizes every undeclared leaf, the Zephyr hardware labs included (RFC-0120 D4).

## Issues homed here (2026-11-07)

| issue | why it belongs here |
| --- | --- |
| [#2305](../issues/2305-user-tier-messages-cite-internal-issue-ids.md) | A user-tier message cites an internal issue id: an adoption stop of the same kind as issue 2332 (internal terms in a First Project build). |
| [#2443](../issues/2443-runtime-log-and-colcon-messages-cite-ids.md) | The same, on the two surfaces 2305 does not reach: runtime logs of a default image and the colcon plugin's errors. |
| [#2468](../issues/archived/2468-bootstrap-builds-no-runner.md) | The book's `just`-free checkout road could not `nros run`: `bootstrap.sh` never built the runner packages. Found by the zenoh probe track, which had no CI lane. |
| [#2347](../issues/archived/2347-scaffold-next-steps-print-nros-sync.md) | `nros new` prints `nros sync` in a Rust package's next steps: an adoption-probe stop from the checkout-track log of the scheduled probe run. |
