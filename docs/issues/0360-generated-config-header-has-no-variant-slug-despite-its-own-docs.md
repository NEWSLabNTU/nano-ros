---
id: 360
title: "`nros_config_generated.h` is written to a FLAT path, not the `<variant_slug>/` its own stub documents — two feature sets overwrite one header, and the sizes it carries are what a consumer compiles against"
status: open
type: bug
severity: medium
area: build
related: [issue-0268, issue-0088, issue-0245, phase-325]
---

## Finding (2026-07-31, wiring phase-325 W1.4)

The checked-in stub at `packages/api/nros-c/include/nros/nros_config_generated.h`
documents a per-variant output path:

> `nros_config_generated.h` is produced per-build by `nros-c/build.rs` and written
> to `$CARGO_TARGET_DIR/nros-c-generated/<variant_slug>/nros/nros_config_generated.h`
> where `<variant_slug>` = sorted underscore-joined cargo feature list.
>
> Build systems pick the right variant header …
> Direct `cargo build`: add `-I$CARGO_TARGET_DIR/nros-c-generated/<slug>`.

The code writes no slug. `packages/tooling/nros-build-helpers/src/c.rs:316`:

```rust
write_header_to_target_dir(
    &["nros-c-generated", "nros", "nros_config_generated.h"],
    &exact_header,
);
```

and on disk:

```
target/nros-c-generated/nros/nros_config_generated.h        # flat
target/nros-cpp-generated/nros/nros_cpp_config_generated.h  # flat
```

There is one file per project, not one per feature variant.

## Why it matters

That header carries **storage sizes** — `NROS_EXECUTOR_SIZE`,
`NROS_SERVICE_CLIENT_SIZE`, the `_OPAQUE_U64S` counts. A consumer compiles its
`_opaque` buffers from those numbers while the Rust side writes into them
according to the sizes it was actually built with. When the two disagree the
result is a silent overflow at runtime, not a build error — the whole reason
issue 0268 (and 0088 / 0245 / 0114 / 0122 / 0123) exist as a family.

The variant slug is precisely what keeps that from happening across feature sets.
Without it:

1. `cargo build -p nros-cpp --features std,rmw-cffi` writes the header.
2. `cargo build -p nros-cpp --features std,safety-e2e` **overwrites the same
   file** with different sizes.
3. Anything still linking the first archive now compiles against the second's
   header.

Nothing detects this. `check-sizes-header-mirrors.sh` compares each build tree's
MIRROR against its source header — it verifies the copy matches the original, not
that the original matches the archive a consumer links.

## How it surfaced

phase-325 W1.4 needs a PX4 module to `#include <nros/init.h>`, which pulls this
header. Wiring `-I$CARGO_TARGET_DIR/nros-c-generated` per the stub's "direct
cargo build" line does not work as documented (there is no slug directory to
name), and the flat path silently works — which is the problem: it works for
whichever feature set wrote last.

Note also that the include must be **prepended**, because
`packages/api/nros-c/include/nros/` ships a same-named stub whose body is the
`#error`. Search that directory first and the stub wins. That part is correct
behaviour — the `#error` is the intended failure when nothing supplies the real
header — but it means include ORDER is load-bearing and undocumented.

## Ways to fix

**A. Implement the documented slug.** Write to
`nros-c-generated/<sorted_features>/nros/…` and have consumers name the variant
they built. Matches the stub, and makes the mismatch impossible rather than
merely unlikely. Cost: every consumer that hardcodes the flat path must learn the
slug — `borrowed_{c,cpp}_e2e.sh`, `heap_compile_check.rs`, `nuttx_ffi_build.rs`
and phase-325's `NanoRosPx4Module.cmake` all reference `nros-c-generated`
directly today.

**B. Stamp the variant INTO the header and check it.** Emit a
`#define NROS_CONFIG_VARIANT "rmw-cffi_std"` plus a matching symbol in the
archive, and have consumers assert they agree. Keeps the flat path; converts a
silent size mismatch into a link error. Cheaper than A and catches the same
class, but leaves two variants still fighting over one file.

**C. Fix the stub's documentation to match the code.** Cheapest, and wrong on its
own — it documents the hazard rather than removing it. Worth doing as part of A
or B so the guidance stops describing a mechanism that does not exist.

**Recommended: B, then C.** A is the "right" answer but ripples through every
consumer; B closes the failure mode that actually bites (silent → loud) at a
fraction of the cost. Whichever lands, C must follow — a stub that documents a
slug nobody writes is how this went unnoticed.

## The general shape

Documentation describing a safety mechanism that the code does not implement is
worse than no documentation: it tells the next person the hazard is handled. Same
family as issue 0354 (a validator with no caller) and 0351 (a stamp that answers
presence rather than truth) — the artifact exists, and what it implies is not
true.
