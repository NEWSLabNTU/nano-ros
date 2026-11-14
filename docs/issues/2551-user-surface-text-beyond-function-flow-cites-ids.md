---
id: 2551
title: "User text that crosses a function, sits in a struct field or an unmarked table, goes through a C++ stream, or is a generated file's rustdoc can still cite internal ids"
status: open
type: tech-debt
severity: low
area: [cli, ux, logging, runtime, build]
related: [issue-2547, issue-2443, phase-523]
found: 2026-11-14
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md) W5.**
The gaps [issue 2547](archived/2547-user-surface-text-formatted-away-or-tabled-cites-ids.md)
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
