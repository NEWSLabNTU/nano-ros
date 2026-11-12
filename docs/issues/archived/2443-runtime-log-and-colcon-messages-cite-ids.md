---
id: 2443
title: "Runtime logs of a default image and the colcon plugin's errors still cite internal ids, and no gate reads them"
status: resolved
type: tech-debt
severity: low
area: [cli, ux, logging, runtime]
related: [issue-2305, phase-523, issue-2547, phase-552]
found: 2026-11-11
resolved: 2026-11-14
resolved_in: "PRs #3784, #3786, #3787, #3789, #3790, #3792, #3794 and #3804 (the runtime-subject widening); gaps filed as issue 2547"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md).**
The two surfaces no gate reads after issue 2305 took CMake's default-level
output to a zero baseline and gated build-script warnings.

## What

RFC-0116 D2 says the user surface names an action, never an internal id. Two surfaces a user reaches are not read by the gate:

- **Runtime logs of a default image.** Measured 2026-11-11 with a string
  scan (literals outside comments and `#[cfg(test)]`, bindgen's
  `generated.rs` left out): about 40 literals across 23 files in
  `packages/{core,api,rmw,platform}` carry an id, the most in
  `zpico-sys/c/zpico/zpico.c` (13) and `nros-rmw-xrce/src/graph.c` (5).
  Not every literal is a log line; some are `assert!` texts or data.
- **The colcon plugin** (`packages/cli/colcon-cargo-ros2/colcon_nano_ros`):
  its refusals are raised in a user's `colcon build`, and at least
  `manifest.py`'s unknown-build-type error ends "— RFC-0087 D2."

## Fix direction

The same as issue 2305's: move each id into a comment above the message, and
add both surfaces to `check-user-surface-ids`. The runtime subject needs a
rule for which literals are log and panic text (an `nros_log` call, a
`printf`/`printk`, a `panic!`/`assert!` message), because a runtime crate
also holds data strings; a C subject needs a string lexer the gate does not
have yet.

## Progress (2026-11-12)

**The colcon plugin is done.** Its two refusals that cited ids now say what
to do: `manifest.build_path` names the `<build_type>` values to write, and
`manifest.deploy` keeps its remedy without the RFC and phase ids, which moved
to a comment above the raise. `check-user-surface-ids` reads the plugin as a
subject: every Python string literal under `colcon_nano_ros/` that is not a
docstring, baseline zero. Negative control: the same gate on the old
`manifest.py` reports its three ids.

**User ruling, 2026-11-12** ("Take your recommendation"), relayed by the
manager: "Error reporting = A NOW, then C later: user-reachable output,
build-stopping refusals included, names what/where/how-to-fix and NO
phase/issue/RFC id. Ids live in source comments and `-v` detail. (C, stable
codes + an explain verb, goes to 243 later.)" The issue was un-parked on the
same ruling.

**The CLI library refusals named here are done.** `build_type.rs`'s
retired-`<build_type>` warning, and `package_xml.rs`'s two refusals (one build
role per package; the retired `<nano_ros>` element), keep their remedy and
lose the RFC and phase ids, which moved to comments. The CMake twin of the
`<nano_ros>` refusal (`NanoRosPackageXml.cmake`, a `FATAL_ERROR`) loses them
too, so the two readers still say the same thing. `check-user-surface-ids`
reads both Rust files whole (`REFUSAL_FILES`, baseline zero). Negative
control: the gate on main's two files reports their 5 ids.

**Backlog the ruling opens, measured 2026-11-12, not done here:**
- `nros-cli-core/src` outside `cmd/` and `cargo-nano-ros/src` hold about 219
  id-bearing string literals (string scan; not all are messages, some are
  generated code or data). Most in `entity_inventory.rs` (26),
  `builder/cmake_root.rs` (12), `builder/west_app.rs` (12).
- CMake `FATAL_ERROR`/`SEND_ERROR` text cites an id at 103 sites in 52 files.
  `check-cmake-user-output-refs` excludes those two levels on purpose ("a
  build that stops is being diagnosed"). Ruling A reverses that exclusion.

**The backlog is ratcheted (2026-11-12).** The manager assigned the sweep,
behind a ratchet so the count cannot grow meanwhile:
- `check-user-surface-ids` counts ids per file in
  `.config/user-surface-ids-ratchet.json`, 226 in 59 files. That is 209 in the
  CLI library (`nros-cli-core/src` outside `cmd/`, `cargo-nano-ros/src`;
  test-only modules not read) and 17 in build-script panic texts.
- `check-cmake-user-output-refs` counts `FATAL_ERROR`/`SEND_ERROR` text too:
  102 sites in `.config/cmake-user-output-refs.json`.
- A count that rises fails, and one that falls fails until its row is
  lowered in the same change.
- Not counted, a stated gap: build-script text assembled away from its
  panic, and build-script literals that are not panic or warning text (84
  ids, mostly headers of files generated into `OUT_DIR`).

The sweep goes by file, biggest first. Each message names what, where and
how to fix; its id moves to a comment. Stable error codes (ruling C) come
later, so each message stays self-contained, with room for a code prefix.

**Left: runtime logs of a default image.** The rule for which runtime
literals are log or panic text, and the C string lexer, are still to do.

**Shell-script output gated at zero (2026-11-14).** `check-user-surface-ids`
now reads what the scripts a user runs print: `scripts/install.sh`,
`scripts/bootstrap.sh`, `activate.sh`/`activate.fish`, the two scripts a
user's build runs from the release SDK root (`scripts/nuttx/build-nuttx.sh`,
`scripts/check-shared-cargo-dir-used.sh`), every script the user book tells a
reader to run (computed from the book at check time:
`scripts/nuttx/stage-external-apps.sh` today), and everything those source.
It reads `echo`/`printf` text, print helpers such as `die`, `cat <<EOF`
bodies that reach the terminal, and the value of a variable a printer prints
(the rc-file snippet bootstrap appends). It found 24 ids in 9 files; each
moved to a `#` comment, and the line says what to do instead (bootstrap's
missing-CLI refusal now says to update or re-clone, activate.sh's ROS-not-
loaded notice says to source ROS 2 first). The rc snippet's marker comment
lost its id too; no code matches that marker (an install is recognised by the
activate path), so an rc written by an older bootstrap is still recognised.
Gaps, stated in the checker: text a non-printer builds, a heredoc written to a
file, and scripts a user-run script executes rather than sources.

## Progress (2026-11-14)

**Runtime logs of a default image are done.** `check-user-surface-ids` reads
them as the subject "runtime log and panic text", baseline zero, no
allowlist. The rule: in `packages/{core,api,rmw,platform}` (tracked files;
vendored, test, bench and example trees, bindgen's `generated.rs` and build
scripts left out), a string literal counts when its innermost enclosing call
prints it. In Rust that is a log macro (`log::`/`defmt::`/bare
`trace!`..`error!`, nros_log's `log_*!` and `nros_*_throttle!`, `cpp_diag!`,
`cpp_info!`), `println!`/`eprintln!`, `panic!`/`unreachable!`/`todo!`/
`unimplemented!`, `.expect(`, or the message argument of an `assert*!`,
directly or through `format!`/`concat!`; `#[cfg(test)]` and
`#[cfg(all(test, …))]` items are not read. In C and C++ a small lexer
(comments, escapes, char literals) finds the call around each literal:
`printf`/`fprintf`/`snprintf`/`puts`/`printk`/`syslog`/`assert`, Zephyr
`LOG_*`, PX4 `PX4_*`, zenoh-pico `_Z_*`, the rclcpp/rclc/nros log macros and
the backends' own helpers (`xrce_log_error`, `graph_log`,
`nros_threadx_stderr`, `nros_freertos_append`). A table entry, a compared key
or an assert's condition sits in no such call and is not read. The scan
measured 101 ids in 33 files (most in `nros-node`'s `arena.rs`, 18, then
`nros-cpp/src/lib.rs` and `spin.rs`, 10 each); each id moved to a `//`
comment above its call and each message kept its remedy.

Gaps, stated: text built away from its print (a `write!` into a `Display`
impl: 8 literals, in `nros-orchestration-ir`, `nros-node` and
`nros-baremetal-common`; and messages
assembled and logged later); and calls the lists do not name. Compile-time
text is the next subject, below.

**Compile-time text is gated at zero (2026-11-14).** `check-user-surface-ids`
reads what the compiler shows a user building nano-ros or an app against it,
in `packages/`, `zephyr/` and `cmake/` outside vendored, test, bench, example
and fixture trees: C/C++ `#error`/`#warning`/`#pragma message` text and the
strings of `static_assert`, `_Static_assert`, `[[deprecated]]`,
`__attribute__((deprecated|error|warning))` and the `*_DEPRECATED*` wrapper
macros; Rust `compile_error!`, `#[deprecated]`, `#[must_use = …]`,
`#[diagnostic::on_unimplemented]`, the message of an assert in a `const`
item or block, and a proc-macro crate's `syn::Error` text. It shares the
runtime subject's lexer. Beyond what the runtime-log fixes had already
cleared, it found 97 ids in 34 files: the `>= 1` array-size `#error`s, the
XRCE buffer and fragment refusals, the FreeRTOS static-allocation and
time-slicing refusals, the renamed-API deprecation notes and seven
`nros::main!` refusals. Each id moved to a `//` comment above its construct,
and several messages gained the fix they lacked (the knob to raise, the
`prj.conf` or defconfig option to set, the name to use instead; the
executor's arena asserts now name the `nros-node/build.rs` term to raise).
Stated gaps: a message built away from its construct (a `String` handed to
`Error::new` later), a proc macro's `panic!`, and a `const fn` assert
reached only from a const context elsewhere.

**Next, in order (manager, 2026-11-14).** Ruling A covers build-stopping
refusals, so the runtime subject's stated gaps are in scope too, each as its
own gate subject starting at baseline zero: first compile-time text
(`#error`, `static_assert`, `compile_error!`, `#[deprecated]` notes and
proc-macro errors, well over 100 ids), then text assembled away from its print
(`Display` impls and messages built for a later log call).

## Progress (2026-11-14): text assembled away from its print

**Error text built away from its print is gated at zero.**
`check-user-surface-ids` adds the subject "assembled error text": in every
Rust crate under `packages/` (the runtime subject's skips; the CLI library is
read whole by its ratchet already), (a) every literal in the body of an
`impl Display for` or `impl Error for`, (b) the strings of thiserror's
`#[error(…)]`, and (c) a literal handed to `Err(…)`, `bail!`/`eyre!`/
`anyhow!`/`ensure!`, `.context`/`.with_context`/`.wrap_err`,
`.ok_or`/`.ok_or_else`, `.map_err` or the arena oracle's `.text(…)` builder,
directly, through `format!`/`format_args!`/`concat!`, or through an error
enum's variant. A `Debug` impl is developer-facing (`{:?}`) and is not read.
The directory choice follows the callers: the orchestration, tooling, board
build-helper, runner and CLI-side crates (`nros-orchestration-ir`,
`nros-platform-config`, `nros-board-common`, `rosidl-*`, `nros-launcher`,
`nros-build`, …) return their refusals as `Err(String)` to a build script that
panics with them or to the CLI that prints them. The scan measured 61 ids in
32 files (`platform_config.rs` 10; `target_exec.rs`, `lib.rs` and
`leaf_system.rs` of `nros-orchestration-ir` 4 each; the 8 `Display` literals
the runtime subject named, and a ninth in `rosidl-codegen`'s `bounds.rs`; four
`#[error]` strings, two in `rosidl-codegen` and one each in `rosidl-lower` and
`nros-orchestration-ir`; the arena oracle's two `.text(…)` pieces). Each id moved to a `//` comment above its literal; a few messages
gained the remedy the id stood in for (add the platform to a descriptor's
`names`; give the field another storage mode; drop `class`). Unit tests that
asserted an id in the text now assert the sentence instead.

With this subject every user-facing text class this issue named is gated:
runtime logs, the colcon plugin, compile-time text and assembled error text.
Stated gaps: a message formatted into a variable first and handed over later
(`let why = format!(…)`: about 30 such literals still carry an id, most in
`nros-orchestration-ir`'s `levels.rs` and `rtos_realizer.rs` and in
`nros-launcher`), a table of message strings (`retired_tables.rs`,
`nros-entry-lower`'s board hints), a literal a function returns for its
caller to print, generated-file headers, and C text built into a buffer by
anything but `snprintf`. Measured on the way: the runtime subject reads only
`packages/{core,api,rmw,platform}`, so the log, print and panic text of
`packages/boards`, `tooling` and `runners` is unread (27 id lines today,
most in `nros-board-common` and the Zephyr and FreeRTOS board crates).
Recommendation: do not archive yet. Widen the runtime subject's directories
(the shell-script subject has landed, above); then the remaining gaps are the
stated ones above, and the issue can be archived.

## Progress (2026-11-14): every package and example

**The runtime subject reads every package and every example.** It read
`packages/{core,api,rmw,platform}` only; it now reads all of `packages/`
and `examples/` (an example is code a user copies out and runs), with the
same skips (vendored, test, bench and build trees, an example's own `tests/`,
bindgen output, build scripts) plus the CLI library and `cmd/`, which their
own subject reads whole. The call lists gained the board print helpers: the
`Board` trait's `println` (`B::println(format_args!(…))`), `print_line`,
`hprintln!` and `nros_nuttx_println!` in Rust; `uart_puts`,
`semihosting_write0`, `nros_board_log`, `nros_board_freertos_console_write`,
`board_stderr_line` and the other board console helpers in C, and the
Cyclone DDS backend's `NROS_CYC_TRACE`. The build-script subjects read
examples' build scripts too. The scan found 37 runtime ids in 21 files, all
in `packages/boards` and `packages/tooling` (`base_config.rs` 5, the FreeRTOS
`entry.rs` 4, the Zephyr `entry_sched_tasks.rs` 3), one more in the FreeRTOS
`entry.rs` passed through a conditional, and 6 build-script panics in the
mps2-an385 C++ examples; the examples' printed text held none. Each id moved
to a comment above its call; refusals gained their remedy (regenerate the
entry with `nros build`, set `NROS_C_INCLUDE`, depend directly on nros-c and
nros-cpp).

## Resolution

Every text class this issue named is gated at baseline zero by
`check-user-surface-ids`, with no allowlist: the colcon plugin (#3593), the
CLI library and build-script panics as a hard rule after the ratchet sweep
(#3617 to #3780, then #3784), CMake configure messages (#3786, #3787),
runtime log and panic text (#3789, widened to every package and example by
#3804), user-run shell scripts (#3790), compile-time text (#3792)
and text assembled away from its print (#3794). The gaps the checker states
(text formatted into a variable or kept in a message table and printed
later, literals a function returns for its caller to print, generated-file
headers, C text kept in message macros) are measured and tracked by
[issue 2547](2547-user-surface-text-formatted-away-or-tabled-cites-ids.md),
with a gate plan.
