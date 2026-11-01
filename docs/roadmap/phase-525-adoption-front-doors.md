# Phase 525 — Adoption front doors: an action-only user surface, a single package, one run verb, serial first

**Status (2026-11-01). IN PROGRESS — W2 done; W1, W3 and W4 pending.**
W1 waits on phase-526 W5, W3 is carried by phase-528 W4, and W4 needs both.
Implements
[RFC-0116](../design/0116-adoption-purposes-and-front-doors.md) D2, D5, D6
and D7 on QEMU mps2-an385 first, so nothing here waits on hardware or on an
emulator twin. Independent of phase-522 (the registry) and phase-524 (the
emulator twins); [phase-523](phase-523-education-labs-adoption-probes.md)
depends on W3 here.

---

## Why

RFC-0116's problem section lists six frictions every adopter meets. Four of
them are about the front door itself, not about which board is behind it: the
user surface speaks in internal ids, the first page asks for six concepts
before one publish, "run what I built" has four spellings, and the first
chapter asks for a network before a cable. All four can be fixed and measured
on the QEMU board the tree already probes weekly, which is why this phase comes
first and carries no board work.

The measure that matters is the field budget of RFC-0116 D1: 30 minutes from
an unopened box to `ros2 topic echo` showing the board's data. On QEMU the box
is the container, so the probe can time it today.

## Work items

### W1 — `check-user-surface-ids` and the sweep (D2)

**Sequenced after [phase-526](phase-526-cli-two-tier-restructure.md) W5
(2026-11-01).** The command tree is restructured first (RFC-0117), and W5
there moves the help text's ids into `//` comments. This item then adds the
gate and sweeps the refusal strings and the book, once, over the final verb
names.

- Write `scripts/check/check-user-surface-ids.py`. Its subject is the rendered
  user surface: it runs `nros --help` and every verb's `--help`, walks the
  refusal strings reachable from `packages/cli/nros-cli-core/src/cmd/` (the
  `anyhow!`/`bail!` texts and the `Refusal` builders), and scans
  `book/src/getting-started/`. It fails on any `phase-NNN`, `Phase NNN`,
  `issue NNNN`, `#NNNN` or `RFC-NNNN` token, with the same contributor-block
  exemption `check-book-no-just` uses.
- Sweep every hit. A verb's help says what the verb does; the phase or issue
  that built it moves to the doc-comment above the string. A refusal names the
  next command. A chapter sentence that cites an issue either states the fact
  the issue holds or moves the citation into a contributor block.
- Wire the gate onto the fast line.

Acceptance: the gate runs in `just check fast` with a baseline of zero and no
allowlist; `nros --help` contains no internal id; the sweep's commit message
carries the sweep command.
Carried from [phase-522](archived/phase-522-support-registry-primitives.md) W5:
the gate's subject includes `nros board list`, `nros board show` and
`nros doctor --report`, and it finds no tier number (`tier 1`, `tier 2`,
`tier 3`) in their output.

Decision step: if the sweep finds a refusal whose only honest remedy is "read
issue N" (the fix is not a command the reader can run), that refusal states
the fact in a sentence and the issue is filed as a front-door defect, not
exempted.

### W2 — The single-package scaffold (D6) — DONE (2026-11-01)

Landed: `nros new <name> --board <board>` reads the platform from the board
catalog and writes one package — one `[image.<id>]` naming the board, the
`--template` node, the manifest, a `README.md` rendered from
`packages/cli/cargo-nano-ros/templates/single-package/README.md` (its build
lines are the ones `nros new` prints), and a `.gitignore` for what sync and
build write. The Rust self-bringup no longer writes a leaf `nros.toml`:
nothing read it (measured: no reader under `packages/`, `cmake/`). Its
`.cargo/config.toml` went in the same window by phase-528 W2, which moved the
QEMU runner into `[board.run]`; this item measured the same thing first (the
scaffold with both files deleted syncs, builds the mps2-an385 image and prints
its QEMU run line), so the two agree. `nros new
<ws> --workspace --from <package>` carries the node into a workspace
unchanged, as D6 says: `src/lib.rs` and `package.xml` move file for file to
`src/<package>/`, and the package's `system.toml` moves byte for byte to the
bringup `src/<package>_bringup/` (its `[[component]]` row already names the
node). What stays behind is what made the package its own image: `main.rs`,
the manifest's `[[bin]]` table and generated selection dependency, the README,
and everything sync and build wrote. A first version kept the package whole
as its own bringup; it built green and linked no second node (`nm` found no
`second_node` symbol in the image), because a package that is its own image
stays a leaf, so the workspace road never saw it. The bringup also gets
`launch/system.launch.xml`, one `<node>` per component (the file the single
package's sync synthesised under `build/`), because a workspace reads an
authored one. `nros new node` then declares the second node in the bringup
and, new here, adds its `<node>` line to that launch file, so the node runs
and is not only declared. Growing a C or C++ package is refused for
now, naming why: its scaffold is an executable with its own `main`. The same
run found `nros new node`'s manifest missing the `macros` feature its own
`lib.rs` needs, so a fresh node package failed to compile on its own; fixed
here. `check-template-copy-out` scaffolds `--board linux` and `--board
mps2-an385-baremetal` outside the checkout, checks the single-package shape
(with a self-test that plants each forbidden file), and builds them; it also
gained a cargo-leaf arm for a cross board, where a missing toolchain is the
build's own preflight refusal, reported NOT VERIFIED. `--platform` keeps
working unchanged, and no flag was renamed.

Measured before/after (`nros new sensor_node … --template talker`):

| invocation | before | after |
| --- | --- | --- |
| `--board mps2-an385-baremetal` | refused: requires `--platform` | `Cargo.toml`, `package.xml`, `system.toml`, `src/{lib,main}.rs`, `README.md`, `.gitignore` |
| `--platform baremetal` | the above plus `.cargo/config.toml`, `nros.toml`, no README | as `--board`, with the platform's board key |
| `--board linux` | refused: requires `--platform` | `Cargo.toml`, `package.xml`, `system.toml`, `src/{lib,main}.rs`, `README.md`, `.gitignore` |

The README is the first chapter's text in the sense W4 needs: W4's chapter
includes that template rather than restating it.

Left open, and a decision for the maintainer rather than this item:

- A Rust package on FreeRTOS, NuttX or ThreadX is still refused by the
  scaffold (it prints the workspace commands), although the in-tree single-
  package examples (`examples/mps2-an385-freertos/rust/talker` and siblings)
  show the cargo road builds that shape today. Adding the scaffold template for
  it is a contained follow-up; it was left out to keep this change off the
  runner text another session is editing in `scaffold.rs`.
- A Rust package on Zephyr is a west leaf with a nested node crate. Per the
  decision step below, the scaffold should still write one package and the
  rest be synthesised under `build/`; that synthesis is `nros run` /
  `nros build` work (phase-528), and until then the scaffold keeps refusing
  with the workspace commands. C and C++ on Zephyr already scaffold one
  package.
- The C/C++ project scaffold still writes a leaf `nros.toml` that nothing
  reads.

- `nros new <name> --board <board> --template talker` writes one package: a
  `system.toml` with one `[image.<id>]` stating the board, one node from the
  template, and nothing else the reader edits. The generated build
  configuration RFC-0098 already renders; no `.cargo/`, no workspace root, no
  bringup, no launch file.
- `--template` keeps today's set (talker, listener, server, client,
  action-server, action-client).
- `nros new --workspace` learns `--from <single-package>` so the package grows
  into a workspace unchanged when the reader wants a second node.
- The scaffold's README is the first chapter's text, not a second explanation.

Acceptance: a copy-out of the scaffolded package builds under
`check-template-copy-out`; the package resolves with no `nros sync` of its own
beyond what `nros run` does; the field chapter (W4) names no workspace word.

Decision step: if a board's road cannot build a single package (a west leaf
needs the workspace shape, say), the scaffold still writes one package and
`nros run` synthesises the rest under `build/`, never in the reader's tree.

### W3 — `nros run` (D7)

**Carried by [phase-528](phase-528-run-on-host-and-emulator.md) W4
(2026-11-01).** RFC-0118 amends RFC-0116 D7: `nros run` covers host and
emulator targets and refuses on hardware, naming `nros flash --monitor`
(phase-529). The acceptance below is phase-528 W4's, and this item is marked
done when that is.

- `nros run [--expect <line>] [--timeout <s>]` in the package or workspace
  directory: builds if stale (the same preflight `nros build` runs), then
  executes the next step `nros build` prints for the image's board — the
  emulator command, the host binary, or the flash command followed by the
  console — and streams the console.
- `--expect` exits 0 on the line and non-zero on timeout, printing the last
  lines of console. This is the lab grading command and the probe's assertion.
- The run command comes from one place: the board descriptor and, once
  phase-522 lands, the registry row. `nros run` adds no spelling of its own;
  the issue 2169 test is extended so the verb, the printed next step and the
  chapter's line are one line.

Acceptance: `nros run --expect 'Publishing: 0'` passes on the mps2-an385 QEMU
talker and on the Linux talker; the probe harness calls it in place of the
chapter's hand-spelled QEMU line; the 2169 test covers the verb.

Decision step: a board whose run cannot be expressed by `nros run` (no
emulator, no flash runner, no host binary) does not get a getting-started
chapter (RFC-0115 D4); it stays on the evidence page as build-only.

### W4 — The serial-first first chapter on QEMU, timed (D5, D1)

- Rewrite the first getting-started chapter around W2 and W3 on mps2-an385
  QEMU: install, `nros new … --template talker`, `nros run`, `ros2 topic echo`.
  The image dials the router over the serial link (QEMU's UART to a host pty),
  not over the user network; the network chapter follows it.
- Every block is probe-tagged; the probe prints the wall-clock time from the
  first `nros setup` to the asserted line and fails when it exceeds the field
  budget.
- The chapter states, in one sentence each, what it assumes (a Linux host or
  the documented container) and what is deferred (WiFi, Zephyr 4.4).

Acceptance: `just probe <track>` passes with the time printed; the time is
under 30 minutes on the probe runner; the chapter names no workspace word and
no internal id (W1's gate).

Decision step: if the budget is missed with W2 and W3 in place, RFC-0116 D6
widens to a generated single-file project and this phase gains W5 for it.
If the serial link cannot be driven on QEMU mps2-an385, the chapter opens
over the emulator console transport and RFC-0116 D5 records the exception.

## Acceptance

| Work item | Industrial | Field solutions | Education |
| --- | --- | --- | --- |
| W1 | refusals name actions, so a conformance log is readable without the tree | errors name the next command | students can paste an error and get a command, not an id |
| W2 | — | one package, one file to edit | lab 1 scaffold |
| W3 | one smoke command per image | one verb to build and watch | the grading command |
| W4 | — | 30-minute budget measured on QEMU | the first lab, timed |

## Out of scope

Boards and emulator twins (phase-524), the registry and evidence page
(phase-522), the lab series and adoption probes (phase-523), Windows or macOS
native toolchains (the container path is documented instead), ESP32.
