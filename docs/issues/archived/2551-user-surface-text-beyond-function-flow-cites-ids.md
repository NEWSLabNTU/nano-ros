---
id: 2551
title: "User text that crosses a function, sits in a struct field or an unmarked table, goes through a C++ stream, or is a generated file's rustdoc can still cite internal ids"
status: resolved
type: tech-debt
severity: low
area: [cli, ux, logging, runtime, build]
related: [issue-2547, issue-2443, phase-523, phase-552]
found: 2026-11-14
resolved: 2026-11-14
resolved_in: "PR pending (the readings and their sweep, one change)"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md) W5.**
The gaps [issue 2547](2547-user-surface-text-formatted-away-or-tabled-cites-ids.md)
left stated when it closed. User ruling A (2026-11-12): user-reachable text
names what, where and how to fix, and carries no phase, issue or RFC id; ids
live in source comments.

## What

`check-user-surface-ids` reads every user-text shape issues 2443 and 2547
named, at baseline zero. Its "text that reaches a print later" subject follows
a literal within one function only, and recognises tables by name or marker.
These shapes reach a user and no subject reads them:

- **Flow across functions.** A function returns a message (or a value built
  from one) and its caller prints it. The subject reads the literal only at
  its `return`, and not at all when the function returns it bare
  (`fn hint() -> &'static str { "…" }`).
- **A buffer passed in.** A helper writes into a `&mut String` / `fmt::Write`
  its caller prints.
- **Struct fields.** `Refusal { hint: "…", .. }` or a builder field that a
  `Display` impl or a printer reads later.
- **A `Vec` drained into a print.** Lines pushed into a collection and printed
  in a loop.
- **Unmarked tables.** A message table whose name has none of the convention
  suffixes and that carries no `// user-surface: table` marker.
- **C text built into a buffer** by anything but `snprintf`, and **C++ stream
  output** (`std::cerr << "…"`, `RCLCPP_*_STREAM`).
- **A generated file's body.** Build scripts write `///` doc comments above
  the constants they emit into `OUT_DIR`, and several cite ids (most in
  `nros-node`'s build script, about 40 lines; also `nros`, `nros-params` and
  the zenoh RMW's). Those are the rustdoc a user reads for the generated
  constants (`cargo doc`, an IDE hover), so they are user-visible. The
  tracked `packages/interfaces/generated/humble` copies also still hold an
  old header text with ids, though the emitter no longer writes it.

Counts are approximate until a subject reads them; measuring is the first
step below.

## Gate plan

Extend the "text that reaches a print later" subject rather than add a sixth:

1. **Interprocedural, one level.** Index every function in the subject's files
   whose return value (or `&mut String` / `fmt::Write` parameter) carries a
   literal. A call to such a function whose result reaches a sink the subject
   already knows (print, log, panic, `Err`, `bail!`, `.context`, return of
   the caller) marks the literal as read. One level catches the common
   `hint()` / `remedy()` helpers; deeper chains stay a stated gap.
2. **Struct fields.** A literal assigned to a field of a type that implements
   `Display` or `Error`, or whose type name ends in `Refusal`, `Diagnostic`,
   `Message`, `Hint` or `Note`, is read.
3. **Collections.** A `Vec<String>` binding whose elements reach a sink inside
   a `for` loop is a sink for its `push` arguments.
4. **Tables.** Any `const`/`static` array of `&str` tuples whose rows are
   printed through an index or iterator in the same file; otherwise require
   the marker, and list the unmarked tables the measure step finds.
5. **C++ streams.** `<<` chains whose left operand is `std::cerr`,
   `std::cout`, `std::clog` or an `RCLCPP_*_STREAM` macro argument.
6. **Generated rustdoc.** In build scripts, a literal whose text is a `///` or
   `//!` doc line written into an `OUT_DIR` file is read (the comment header
   reading already treats the emitted file's leading comments this way; this
   extends it to doc comments anywhere in the emitted text). Separately, add
   a drift check that regenerates `packages/interfaces/generated/humble` and
   compares, or delete the tracked copy if nothing reads it.

Start as a per-file ratchet for the new readings
(`.config/user-surface-ids-deferred-wide.json`), sweep the biggest files, and
make it a baseline-zero rule in the change that empties it, as 2443 and 2547
did. What one level of flow still cannot see becomes the checker's stated
gap, and a tracked issue if it holds user text.

## Progress

**2026-11-14 — six readings, measured, and the sweep that empties them.**
The "text that reaches a print later" subject (`deferred_hits()`) gained the
six readings of the gate plan, each with its rules, false-positive control
and a self-test both ways, and leaves any literal another subject already
reads to that subject:

- **(e) One level across functions.** A function whose return type is text
  (`str`, `String`, `Cow`, `fmt::Arguments`, `impl Display`, also inside
  `Option`/`Result`) and whose tail or `return` yields a literal (bare, or
  through `format!`/`concat!`/`String::from`/`Some`/`Ok`/`Cow`/
  `.unwrap_or`), or that writes one into a `&mut String` / `fmt::Write`
  parameter. A call marks those literals read when its result reaches a sink
  (print, log, panic, error constructor, a binding that does, a printing
  `for` header, an `impl Display`/`Error` body, the caller's own text
  return), or when the buffer it is handed is a binding that does. Control: a
  literal compared, matched on or bound inside the tail is not the value; the
  callee is resolved by name to the one definition in the caller's file, else
  its crate, else the subject, and a name defined twice is not resolved; a
  method matches only a method call.
- **(f) Struct fields.** A field value in a struct literal (directly or
  through those wrappers) of a type whose path ends in `Refusal`,
  `Diagnostic`, `Message`, `Hint` or `Note`, or that has an `impl Display`/
  `impl Error` anywhere in the subject.
- **(g) Collections.** The header of a `for` loop whose body hands a loop
  variable to a sink is a sink, so a collection's `push`/`extend` arguments
  are read when it is iterated there (folded into the dataflow reading).
- **(h) Tables by use.** A `const`/`static` array or slice of `&str`
  (tuples too) is read whole when a use in its file reaches a sink; in C, a
  `const char *NAME[]` indexed inside a print. Test-only uses do not count.
  `--list-tables` lists the id-bearing `&str` tables no reading reads.
- **(i) C++ streams.** A literal after `<<` in a chain headed by
  `std::cerr`/`cout`/`clog`; an `RCLCPP_*_STREAM` argument is the runtime
  subject's already.
- **(j) Generated rustdoc.** The `///`/`//!` lines a Rust literal emits (as
  the file holds them) are read; its code and plain `//` lines are not.

The dataflow reading was tightened on the way: a function with no return
type returns no text, and a tail `for`/`while`/`loop` is no value.

Measured on the tree before the sweep: **68 ids in 6 literals of 5 files**,
every one generated rustdoc (j): `nros-node`'s build script 32 (the executor
and arena-model consts and the boot-record words), `nros-rmw-zenoh`'s build
script 24, `nros-params`'s 6, `nros-zpico-build` 5, `nros`'s 1. Readings
(e)-(i) found no id: the shapes they read hold none today, and each is
pinned by its self-test and an injection. The sweep moved each id to a `//`
comment above the `format!` that emits it and kept the doc line saying what
the const is and how it is set (`Reading it the other way is issue 1450`
became what reading it the other way does: undersize the store). Three
id-bearing texts the readings still miss were fixed in the same pass: the
`QOS_PROFILE_CLOCK` provenance (it renders as that const's rustdoc through
`qos_profiles!`), a plain `//` line `nros-node` writes into
`nros_executor_backing.rs`, and one `nros-build-helpers` writes into
`nros_cpp_ffi_config.rs`. Measured after: **0**. What is left unread holds
no id in user text: the two `nros-sizes-build` knob tables `--list-tables`
names are data a test reads.

Started as the per-file ratchet the plan proposed, the readings reached zero
in the same change, so they land as part of the baseline-zero rule with no
counted file (`.config/user-surface-ids-deferred-wide.json` was never
committed). The subject costs about 2.5 s of the gate's 10 s (it was about
9.7 s before).

**The tracked `packages/interfaces/generated/humble` tree.** It is read:
six root workspace members, `nros-diagnostics`' and the test harness's
dependencies. It cannot be deleted. Regenerating it here (the CLI built from
this tree, `/opt/ros/humble`, `rustfmt`) differs from the tracked copy in
324 lines over 28 files: the header and rustdoc text of an older emitter (the
ids this issue names), `NROS_EMITTED_CODEGEN_VERSION` 18 where the emitter
writes 21, a newer `cyclone_schema_shape` in the bounds inventories, and,
from the host's newer Humble, `ament_version` and eight more `rosgraph_msgs`
types. A byte-for-byte drift check is therefore host-dependent and would fail
today; regenerating it belongs with the codegen-version question, not with
this gate. That drift is the one remaining place with ids in user-read text,
and goes to a follow-up issue.

## Resolution

Every shape this issue named that is a literal in the tree is read by
`check-user-surface-ids` at baseline zero, with no allowlist: text returned
by a function or written into a buffer it is handed (one level), a message
struct's field, a collection or a `&str` table a loop or a print reads, C++
stream output, and generated rustdoc. The sweep took the 68 ids those
readings found (and 3 more texts they miss) to zero. The readings' remaining
gaps are stated in the checker (deeper flow, ambiguous names, assigned
fields and builders, iterator chains, cross-file tables, C buffers, emitted
`#[doc]` and plain comments) and hold no id in user text today. The tracked
`generated/humble` tree's drift from its emitter does hold such text; it is
filed as a follow-up issue rather than checked here.
