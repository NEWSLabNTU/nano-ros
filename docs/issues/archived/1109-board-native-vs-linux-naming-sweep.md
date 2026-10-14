---
id: 1109
title: "77 `board = \"native\"` selectors predate the native/linux split — some are true, some are claims the board cannot make"
status: resolved
type: tech-debt
area: docs, tooling
severity: low
found: 2026-09-06
related: [1107, 1108, phase-401, phase-445, phase-500]
---

**Home: [phase-500](../../roadmap/archived/phase-500-linux-host-name.md)** — re-homed
2026-10-14 from phase-445 (re-homed there 2026-10-12 from phase-401, which is
archived).

**Status (2026-10-14). RESOLVED by phase-500 W1–W4** (#2139, #2161, #2191,
and the W4 move of `examples/native/` to `examples/linux/`). The board, lane,
module, scope, test binaries, image ids and the host example directory all say
`linux`, each held by `check-host-platform-vocabulary`. W5 (retiring the
deprecated `native`/`posix` aliases) stays on phase-500; it is cleanup, not
this issue. Original status: OPEN, partially done by phase-500 W1; closes with
phase-500 W4. The maintainer REJECTED this issue's 2026-10-13 resolution (the
rule "a host image writes `native` unless it authors a Linux-only fact") and
ruled on the words instead:

- **native** — every general-purpose host (Linux, macOS, Windows);
- **posix** — POSIX-compliant systems;
- **linux** — Linux specifically.

The host board builds on Linux only — measured: `apply_tier_affinity`'s
`cpu_set_t` / `sched_setaffinity` fail on darwin, windows AND freebsd
(FreeBSD's type is `cpuset_t`; the text below says libc defines it for freebsd,
which was wrong). So under these definitions `native` claimed more than the
board delivers, and `linux` is its one name. Phase-500 W1 dropped `native` as a
board selector (refused with a hint), rewrote every tracked selector, deploy
token, scaffold and sdk-index key to `linux`, and gated it
(`check-host-platform-vocabulary`). Left for this issue: the `native` lane /
`just native` module / `setup native` scope / `native_*` test names (W2, with a
deprecated alias), the `[image.native*]` ids (W3), `examples/native/` →
`examples/linux/` (W4); W5 retires the aliases. The sections below are the
history, kept as written.

# Two words, one selector, and nobody has said which is meant where

CLAUDE.md's naming section settles what the three words mean:

- **native** — ROLE: this is the HOST build, not a cross build.
- **posix** — REACH: works on any POSIX-compliant system.
- **linux** — REACH: works only on Linux.

`packages/boards/linux/nros-board.toml` declares `names = ["native", "linux"]`,
and the lookup accepts either (`names.iter().any(|n| n == board)`, in
`cmd/setup.rs:410` and `orchestration/board_descriptor.rs:713`). So both spell
the same board and nothing is broken.

What is unsettled is which word each CALL SITE means. Measured 2026-09-06:

| | count |
|---|---|
| `board = "native"` in a `system.toml` | 77, across 18 files |
| `[image.native]` image names | 18 |
| `board = "linux"` anywhere | **0** |

Zero. The `linux` spelling the board offers has no users in the tree, so every
site is inheriting the older word rather than choosing it.

## Why it is worth a sweep rather than a rename

The two are not synonyms and a blanket substitution would be wrong. A site that
means "build this for whatever host I am on" is correctly `native`. A site that
means "this runs on Linux" should say `linux` — and that is a real claim about
this board, because `nros-board-linux::apply_tier_affinity` calls
`sched_setaffinity` with `cpu_set_t`/`CPU_SET` ungated by `cfg(target_os)`, and
libc defines those for linux/android/freebsd/dragonfly/fuchsia/cygwin and NOT
apple. The crate does not build on macOS.

So the sweep is per-site judgement, not `sed`. The first consumer to write the
`linux` spelling is out-of-tree
([`orin-spe-heartbeat`](https://github.com/jerry73204/orin-spe-heartbeat)),
where the image is a Linux host build and says so.

## Not proposed here

Deleting `native` from the board's `names`. It is the honest word for a host
build and several sites mean exactly that; the defect is that no site has had to
choose.

## Shape of a fix

Read the 18 files, decide per image whether it means the ROLE or the REACH, and
write the word that is true. A gate is probably not worth it — "which did the
author mean" is not statically recoverable — but a note in the naming section
saying the two are both live and must be chosen deliberately would stop the next
site from inheriting.

## Resolution (2026-10-13 — SUPERSEDED by phase-500)

The selector rule below (`native` unless a Linux-only fact) was rejected on
2026-10-14. The key-table half (one `HOST_BOARD_KEYS` list asked by every host
predicate, held by `board_key_table.rs`) survived into phase-500 W1.


### What was measured first: `linux` was not a working name

The premise above — "the lookup accepts either … so nothing is broken" — held
for the board CATALOG only. Every consumer downstream of it had its own key
list, and none had moved when the descriptor did (`["linux", "native",
"posix"]` → `["native", "linux"]`, phase-401/437):

| consumer | knew | so `board = "linux"` |
| --- | --- | --- |
| `nros_entry_lower::BOARD_KEYS` (C/C++ entry pack, tier key) | `native`, `posix` | C/C++ entry: unknown board; Rust tiers: no RTOS sub-table |
| `nros_orchestration_ir::BOARD_PATHS` (Rust `nros::main!`) | `native`, `posix` | no board ZST — the entry does not resolve |
| `executor_sizing::board_honors_entry_sizing` | `"native" \| "posix"` | per-entry sizing dropped |
| `bridge_gen::cyclone_links_via_cargo` | `"native" \| "posix"` | a Rust Cyclone entry loses its `-sys` cargo dep |
| `planner::plan_target_is_hosted` | `"native" \| "posix"` | saved only by the triple containing `linux` |

(`builder/entry.rs`'s own test asserted the gap — `macro_board_crate(&["linux"])
== None`, "the fallback chain is what saves it" — the fallback being the
platform word `posix` hitting the stale key.) So `posix` — the name the descriptor had DROPPED as a false reach claim — was
still live in five places, and `linux` — the name it KEPT — in none. Nobody
could write the true word: that, not habit, is why the count of `board =
"linux"` was zero. Fix: one list, `nros_orchestration_ir::HOST_BOARD_KEYS =
["native", "linux"]` and `is_host_board_key()`, which all three predicates now
ask; both tables carry `linux` and no longer `posix`. Held by
`nros-cli-core/tests/board_key_table.rs::the_host_board_keys_are_one_set_everywhere`
(descriptor `names` = `HOST_BOARD_KEYS` = `BOARD_KEYS`' Native rows =
`BOARD_PATHS`' `LinuxBoard` rows = the keys that honor entry sizing). The
existing `a_spelling_in_both_namespaces_names_the_same_rtos` could not see this:
it iterates `BOARD_KEYS`, so a catalog name the table never learned was outside
its reach (the 0196 shape).

### The rule

**An image selects the host board by its ROLE word, `native`, unless the image
ITSELF authors something only Linux supplies; then by its REACH word, `linux`.**

Reach is a property of the BOARD, stated once in its descriptor (and every host
image inherits it — `nros-board-linux` does not build on macOS whatever a
selector says). A selector that restates the board's reach tells a reader
nothing; one that NARROWS it — "this image needs Linux even if the board one
day does not" — is the one thing a reader porting the image must be told. So
`linux` is reserved for that, and `native` claims nothing false anywhere.

It is mechanical because the Linux-only facts an image can AUTHOR are a closed
list. Measured over every host image's own sources (not the board crate's):
none calls a Linux-only API (`sched_setaffinity`, `epoll`, `eventfd`,
`signalfd`, `timerfd`, `/proc`, `CPU_SET`, `__linux__`, …), and one authors a
Linux-only placement: `examples/workspaces/realtime-rust/src/demo_bringup`
declares `[tiers.high.posix] core = 0`, which the host board applies with
`sched_setaffinity` — and `nros_tests::output::LINUX_CORE_PIN_MARKER` asserts
the pin took. That image now says `board = "linux"`, with the reason beside it.

### Counts

| | before | after |
| --- | --- | --- |
| host selectors in tracked `system.toml` (`[image.*]` + `[image_defaults]`) | 159, all `native` | 158 `native`, 1 `linux` |
| files carrying one | 97 | 97 |

Image IDS (`[image.native]`, `native_xrce`, …) are NOT touched: an id names the
IMAGE, not the board, and fixture rows (`image = "native"`), settings paths and
tests key on it. The rule is about the selector.

Generators: `nros new --workspace` (`new_system.rs`), `scaffold_deploy` and the
templates already wrote `board = "native"`. One did not: `nros new <pkg>
--platform posix` (`cargo-nano-ros/src/scaffold.rs`) wrote `board = "posix"`
into `system.toml` — no board's name, which `nros build` refuses — and
`deploy = ["posix"]` into `Cargo.toml`. Both now write `native`: `--platform
posix` names the PLATFORM, the host board is selected by its ROLE name, and a
freshly scaffolded image authors no Linux-only fact.

### Gate

`check-host-platform-vocabulary` (already on the fast line) gained a second
check: every tracked `system.toml` host selector writes `linux` iff the file
authors a fact on its `LINUX_ONLY_FACTS` list (today: a `[tiers.<t>.posix]
core`), else `native`. The host board's names are read back from the
descriptor, and the gate refuses to run if they are anything but `["native",
"linux"]`, so a third name re-opens the rule instead of slipping past it.
Self-test: seven synthetic cases on every run. Mutations, on the real tree:
realtime-rust back to `native` → rc=1 naming the pin; the native Rust talker to
`linux` → rc=1 ("authors nothing only Linux supplies"); restored → rc=0.

What it cannot see: an image that needs Linux for a reason not on the list (its
own `epoll` call). The remedy is to add the fact to the list, never an
exemption.

### Left

Asked of EVERY board rather than the host, "a descriptor's names are all known
to `BOARD_KEYS` with one family" is red today, outside this issue:
`threadx-riscv64` is a descriptor name `BOARD_KEYS` lacks, `BOARD_PATHS` splits
the mps2 descriptor by entry shape on purpose (`rtic-mps2-an385` vs bare
`mps2-an385`), and ESP32 is dormant (issue 1525). The new test is scoped to the
host board and says so.
