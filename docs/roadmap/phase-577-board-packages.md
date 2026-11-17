# Phase 577 — Board packages: one line switches the board

**Status (2026-11-18). OPEN: design in review; no item started. Owner: nros
241.** Opened by the user's approval on 2026-11-18 of an Arduino-like board
experience: switch boards with one line, and let a board package carry
everything needed to build, link, configure, flash, run and monitor. Implements
[RFC-0135](../design/0135-board-packages.md).

Each item is one PR, claimed with `just claim phase-577-wN` before it starts.
No item starts before the user accepts RFC-0135.

## Why

Most of the machinery exists: the board definition already has run, flash and
console tables; `nros run`, `nros flash` and `nros monitor` exist; and a board
is already a package. What is missing is measured in RFC-0135's Motivation:

- an installed board package cannot be found;
- `nros board list` does not list most boards;
- a board package cannot bring its own architecture profile;
- no nano-ros version range exists;
- there is no single pre-build check;
- three common flash methods have no runner;
- examples are not per board.

## Work items

### W1 — Every board, listed

- **Owner:** unassigned.
- **Status:** OPEN
- `nros board list` lists every board key from every root (workspace,
  `$NROS_EXTRA_BOARD_PATH`, in-tree), one row per key, with its package, root,
  platform and support status. This is RFC-0133 D1's shape. Crate-less boards
  (`linux`, `zephyr`) appear.
- **Acceptance:** the listing's keys equal `BoardCatalog`'s keys for the same
  roots, checked by a test.

### W2 — Discovery through the ament index

- **Owner:** unassigned.
- **Status:** OPEN
- RFC-0135 D5:
  - the `nano_ros_boards` resource-index marker, and the install rule that
    writes it;
  - `$AMENT_PREFIX_PATH` as the lowest-precedence board root, also on the
    platform search path for D4;
  - shadowing reported.
- Amend RFC-0087 D8's text in the same PR.
- **Acceptance:** a board package installed into a prefix, with no source
  checkout and no `$NROS_EXTRA_BOARD_PATH`, appears in `nros board list` and
  builds an image. A workspace copy of the same package shadows it, and the
  shadowing is reported.

### W3 — Version range

- **Owner:** unassigned.
- **Status:** OPEN
- The `<nano_ros_requires nano_ros="…"/>` export tag (RFC-0135 D6): parsed,
  shown in `nros board list`, and enforced through W6. A package without the
  tag warns. In-tree packages are exempt.
- **Acceptance:** a package whose range excludes the running version is
  listed as incompatible and refused at build, naming both versions.

### W4 — Architecture profiles from a board package

- **Owner:** unassigned.
- **Status:** OPEN
- A `nros-platform.toml` in a board package contributes `[arch.*]` profiles
  through the platform search path. A name that collides with one already
  loaded is refused, naming both sources (RFC-0135 D4).
- **Acceptance:** an out-of-tree board package with its own profile builds
  an image using it. A package that redefines an in-tree profile name is
  refused.

### W5 — Flash methods as runner packages

- **Owner:** unassigned.
- **Status:** OPEN
- Add the runner packages `nros_runner_openocd`, `nros_runner_probe_rs` and
  `nros_runner_picotool` (RFC-0119 shape). Give each in-tree hardware board
  definition a `[board.flash]` and a `[board.console]`.
- **Acceptance:**
  - each runner's command line is checked with `nros flash --dry-run` against
    a board definition that names it;
  - one hardware flash per runner is recorded where hardware is available,
    otherwise marked manual in the registry (RFC-0114).

### W6 — The pre-build compatibility check

- **Owner:** unassigned.
- **Status:** OPEN
- RFC-0135 D8: one check, run by `nros build` before any build step and by
  `nros check --board <key>` alone. It calls the existing checks and collects
  every verdict, then adds:
  - a memory budget in the board definition;
  - RMW providers' capability needs, stated as facts.
- Each reason carries a diagnostic code (RFC-0129).
- **Acceptance:**
  - switching a working image to an incompatible board prints every reason
    at once, before any compile;
  - a test covers each reason class in RFC-0135 D8's table.

### W7 — One example per board key

- **Owner:** unassigned.
- **Status:** OPEN
- Every in-tree board key gains at least one example. At least these five
  have none today:
  - `mps3-an536`;
  - `s32z270`;
  - `rpi-pico-w`;
  - `qemu-cortex-a53`;
  - `freertos-posix`.
- `nros board check <package>` builds each example, and runs it where the
  board definition names a host or an emulator.
- **Acceptance:** a gate holds that every in-tree board key has an example.
  `nros board check` passes on every in-tree board package.

### W8 — First consumer: the out-of-tree TI R5F board package

- **Owner:** nros 118 (phase-575 W2).
- **Status:** OPEN
- Phase-575 W2's fixture board is built as a board package in RFC-0135's
  shape:
  - a version range;
  - run, flash and console tables;
  - one example;
  - the vendor SDK through its own variable.

  It is found first through `$NROS_EXTRA_BOARD_PATH`, then through the ament
  index once W2 lands.
- **Staged against the items it depends on.** W8 does not wait for them. It
  lands first with what exists, and each check tightens when its item lands:

  | W8 asserts | Until then | Tightens when |
  | --- | --- | --- |
  | the version range refuses an out-of-range nano-ros, and a missing tag warns | the tag parses and is ignored | W3 (parse and listing) and W6 (refusal at build) |
  | the example builds, and runs where `[board.run]` names an emulator, through `nros board check` | `nros build` and `nros run --dry-run` directly | W7 |
  | `[board.flash] method = "openocd"`, checked with `nros flash --dry-run` | only `[board.run]` and `[board.console]` are stated | W5 |
  | found through the ament index from an installed prefix | found through `$NROS_EXTRA_BOARD_PATH` | W2 |

- The BSP stand-in is a "released SDK root" built from public sources (linker
  script, startup code, UART driver), named by the package's own variable,
  so CI needs no vendor download. The core profile is the in-tree
  `cortex-r5f`.
- **Acceptance:**
  - phase-575 W2's acceptance;
  - every row of the table above at its tightened form;
  - W2's installed-prefix test, passed by this package.

### W9 — Terminology sweep

- **Owner:** unassigned.
- **Status:** OPEN
- Apply RFC-0135 D1 across code identifiers where they are user-reachable, CLI
  help, diagnostics and the book. Measured on 2026-11-18:
  - "board crate": 564 occurrences in `packages/`, 30 in the book;
  - "board descriptor": 162 in `packages/`, 19 in the book;
  - "board package": 106 in `packages/`, 34 in the book;
  - "BSP": 56 in `packages/`, 2 in the book.

  "Board descriptor" becomes "board definition". "Board crate" stays only for
  the optional crate. "BSP" is kept only for the vendor SDK layer. "Board
  support" is kept only for the status.
- Book wording goes through the book owner.
- **Acceptance:** a gate refuses the retired terms in CLI help, diagnostics and
  the book, with an allowlist for historical documents.

## Acceptance for the phase

- **Switching takes one line.** A user switches a working project to another
  board by changing one line. The result either builds or is refused with
  every reason (W6).
- **Installed packages work from anywhere.** An installed board package is
  listed and usable from any project with no path configuration (W2).
- **The package carries everything.** The first out-of-tree board package
  holds everything needed to build, flash and monitor its board (W8).

## Review

- The user flow, from creating a project through building, flashing,
  switching board and adding a library, is reviewed by the book owner before
  RFC-0135 leaves Draft.

## Related

- RFC-0064 (board organisation), RFC-0087 (package identity and discovery),
  RFC-0114 (support status), RFC-0115 (emulator-first boards), RFC-0119
  (runner packages), RFC-0133 (discoverability), RFC-0129 (diagnostic codes),
  RFC-0100 (sizing).
- Phase-575 W2: the first consumer.

## Issues homed here

| issue | why it belongs here |
| --- | --- |
