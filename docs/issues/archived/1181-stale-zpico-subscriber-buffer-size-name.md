---
id: 1181
title: "`ZPICO_SUBSCRIBER_BUFFER_SIZE` is documented in four places and read by nothing"
status: resolved
type: bug
area: docs, rmw-zenoh, memory
severity: low
found: 2026-09-07
related: [1125, 0940, phase-412]
resolved_in: "phase-412 — the one-ended knobs sweep (this commit)"
---

## What

The knob that sizes the zenoh `small` payload class is
**`NROS_SUBSCRIBER_BUFFER_SIZE`**. `packages/rmw/zenoh/nros-rmw-zenoh/build.rs`
reads exactly that name:

```rust
let sub_size: usize = env_usize("NROS_SUBSCRIBER_BUFFER_SIZE", 1024);
```

and `zephyr/cmake/nros_cargo_build.cmake` resolves it under that spelling, with
a comment recording WHY it moved:

> phase-403 -- ONE name, the backend-agnostic one, which is also what the
> Kconfig symbol has always been called. This resolved under
> `ZPICO_SUBSCRIBER_BUFFER_SIZE`, and the mismatch was not cosmetic: it is what
> let a live delivery bug hide.

The old `ZPICO_`-prefixed name survives in the docs and in `.env.example`:

```
$ grep -rln ZPICO_SUBSCRIBER_BUFFER_SIZE --exclude-dir=docs/issues .
.env.example
book/src/reference/environment-variables.md
docs/guides/embedded-tuning.md
docs/design/0038-zero-copy-data-transport.md
```

`docs/guides/embedded-tuning.md` gives it in **five** copy-pasteable command
lines, including the memory-tuning recipes for the 256 KB-class boards, and
`environment-variables.md` lists it as a table row with a default. None of them
does anything: a user who exports it gets the crate default of 1024 and no
diagnostic, which is the shape issue 0940 records — a human tuning a knob by
reading a document and the image not moving.

## Not measured beyond the grep

Found while writing issue 1125's `[env]` sidecar, which emits the LIVE name. I
did not build an image with the stale name exported to confirm it is inert; the
build script's own source is the evidence, and there is no `env_usize` or
`rerun-if-env-changed` for the `ZPICO_` spelling anywhere in the tree.

## What closing it looks like

Rename in the four files, or — better, since a document cannot be gated —
teach `config-knob-census` (or a sibling) that a knob NAME appearing in
`book/`, `docs/guides/` or `.env.example` must be read by some build script.
That is the general form: this is one instance, and phase-403 moved several
names.

Note `ZPICO_SUBSCRIBER_LARGE_SIZE` and `ZPICO_SUBSCRIBER_SIZE_THRESHOLD` are
NOT this bug — those two kept their `ZPICO_` names and are read.

## Resolution (2026-09-11)

The stale spelling was worse than documentation: it was also a PRODUCER.
`examples/fixtures.toml` built the `stress-zenoh` large-buffer row with
`ZPICO_SUBSCRIBER_BUFFER_SIZE = "8192"`, and `large_msg`'s
`test_zenoh_e2e_large_receive` says it relies on that buffer. The binary was
built with the default 1024 small class, and the 4096-byte payloads can only
have arrived through the `large` class. The row now sets
`NROS_SUBSCRIBER_BUFFER_SIZE`, and its locator (`build_zenoh_stress_test_large_buf`)
moved in the same commit (#393).

Renamed everywhere a user or a build reads the name: `.env.example`, both env
references, `docs/guides/embedded-tuning.md` (eight sites), RFC-0038, the C,
C++ and Rust API configuration and troubleshooting pages, two rustdoc comments
and the fixture tests. Left as history: archived issues and phase docs. Left
because another open pull request owns the file: two comments in
`nros-node/build.rs`, the `KNOB_CLASS` entry in `config-knob-census.py`, and a
comment in `check-knob-delivery.py`.

**The class, not the site.** The same sweep in reverse found five more
documented knobs with no reader — `NROS_EXECUTOR_MAX_HANDLES`,
`NROS_MAX_SUBSCRIPTIONS`, `NROS_MAX_TIMERS`, `NROS_MAX_SERVICES` and
`NROS_MESSAGE_BUFFER_SIZE`, whose reader was deleted on 2026-03-15 (724099066)
and which stayed in four documents — plus `NROS_LOCAL_IPV4` /
`NROS_LOCAL_IPV4_BYTES` in six Zephyr leaf `[env]` blocks, read by nothing
since phase-169, and a bridge example documented with `NROS_XRCE_LOCATOR`
when it reads `XRCE_LOCATOR`. All fixed.

What closes it is the gate this issue asked for, generalised to both ends:
`just check knob-ends` (`scripts/check/check-knob-ends.py`). A name claimed by
an env reference, `.env.example`, a fixture row or a leaf `[env]` must be read
by a read IDIOM — a mention in a test string does not count, which is why the
first grep-based method called this name live. Positive control: run against
the pre-fix tree it names `ZPICO_SUBSCRIBER_BUFFER_SIZE` and the other seven.
