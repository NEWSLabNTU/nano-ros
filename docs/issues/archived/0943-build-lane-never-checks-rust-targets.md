---
id: 943
title: "The build lane never checks cross Rust targets, so a missing one surfaces 20 minutes in as an unrelated-looking cmake error"
status: resolved
type: bug
area: tooling, testing
related: [issue-0833, issue-0466, rfc-0014, rfc-0062]
---

## What happened

`just build-test-fixtures lane=all` ran for twenty minutes and then failed the
`freertos` stage. The tail of the stage log held nothing but benign newlib
warnings:

    warning: _close is not implemented and will always fail
    warning: _fstat is not implemented and will always fail
    ...
    error: recipe `build-examples` failed with exit code 2

The actual cause was nine hundred lines up, in a cmake CONFIGURE step:

    Rust target `armv8r-none-eabihf` is not installed
      ...corrosion/share/cmake/Corrosion.cmake:47 (find_package)
    Error: configure failed: `cmake -S build/freertos-cyclonedds-s32z270-freertos ...`

The host simply had not been provisioned for the S32Z270 board.

## Why this is a defect and not just "run setup first"

**Issue 0833 already described this exact failure**, verbatim, in the header of
`config/rust-targets.txt`:

> `armv8r-none-eabihf` [...] was added to the installer and never to the
> checker, so `just doctor` reported `[OK] rust-targets` on a host that could
> not configure the FreeRTOS C++ workspace lane at all — corrosion failed at
> CONFIGURE with "Target armv8r-none-eabihf is not installed", make returned 2,
> and the tail of the build log held only benign newlib warnings.

0833 fixed the DOCTOR's copy of the list. It did not put the question on the
path anyone building fixtures walks. So the check exists, is correct, reads the
right SSoT — and is only asked by a command you run when you already suspect
something is wrong. The whole point of `check-tier-preconditions` (issue 0466)
is that preconditions are asked BEFORE the twenty minutes, all at once, each
with its remedy. Rust targets were not among them.

The cost is not the missing target. It is that the log tail accuses the wrong
thing: a reader sees linker warnings about newlib stubs and goes looking for a
toolchain bug in FreeRTOS.

## Fix

`scripts/check-rust-targets-installed.sh`, wired as precondition **0** in
`check-tier-preconditions.sh` — first, because no amount of tree work fixes it
(it is a host-provisioning fact, unlike every other entry, which a pull or a
rebuild can re-arm), and because it is the cheapest probe there.

It reads `scripts/lib/rust-targets.sh`, i.e. the same
`config/rust-targets.txt` the installer and the doctor read. **Not a second
hand-authored copy** — a second copy is the defect 0833 existed to remove, and
this would have been the third.

    [x] cross Rust target(s) declared by this tree are not installed
        remedy: just workspace rust-targets   (or: rustup target add <triple>)
          missing rust target(s): armv8r-none-eabihf

`build-std` rows are excluded deliberately (no prebuilt std; `rustup target
list` never reports them), and the probe fails OPEN when `rustup` is absent,
matching `builder/preflight.rs` — a host managing Rust another way cannot be
probed this way, and guessing would block a working setup.

## Still open, and filed separately

The SDK index has its own `[rust.target.*]` table
(`nros-sdk-index.toml:984-1001`) which `nros setup --check` walks — and it is
**missing `armv8r-none-eabihf`**. That is a third spelling of this list with no
gate between it and `config/rust-targets.txt`; `scripts/check-rust-targets-
covered.py` scans board TOMLs, cmake toolchains and `.cargo/config.toml`, but
not the index. It is 0833's own defect shape, one layer up. See [[issue-0944]].

## Acceptance

* ~~A host missing a declared cross target learns so from
  `check-tier-preconditions`, with a one-command remedy, before any build
  starts.~~ Met.
* ~~The probe reads the existing SSoT rather than adding a copy.~~ Met.
