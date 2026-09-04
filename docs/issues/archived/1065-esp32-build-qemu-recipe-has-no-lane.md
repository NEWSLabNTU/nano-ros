---
id: 1065
title: "RETRACTED — `just esp32 build-qemu` does have a lane; the premise was a truncated grep"
status: wontfix
area: testing
severity: medium
related: [1025, 0196, 0883]
---

## What

`just esp32 build-qemu` is the documented way to produce the ESP32 QEMU flash
images, and **nothing in CI runs it**. Grepping the whole tree for callers finds
only `just docker build-qemu` (a different recipe, in the Docker module) and
prose references.

The ESP32 e2e tests do not use it. `packages/testing/nros-tests/tests/
esp32_emulator.rs` resolves the ELF through the fixture resolver and packs its
own image:

```rust
let elf = build_esp32_qemu_talker().expect("Failed to build esp32-qemu-talker");
let flash_image = nros_tests::build_dir(nros_tests::kind::ESP32_QEMU, &[])
    .join("esp32-qemu-talker.bin");
create_esp32_flash_image(elf, &flash_image).expect("Failed to create flash image");
```

So the tests exercise a SECOND packing path that happens to be correct, and the
recipe a human is told to run is exercised by nobody.

## Evidence that this is not theoretical

Issue 1025: the recipe's ELF lookup passed `"" ""` for the row's cargo args and
env, so it read `build/cargo-fixtures/qemu-esp32-baremetal/` while the build
wrote `qemu-esp32-baremetal-4118800323`. **No flash image could be packed at
all**, and it stayed that way through many green CI runs, because the only
consumer of the broken path is a person at a terminal.

It also hid a second time: on a developer machine that had built those rows
before they gained an `env`, a stale ELF sits at the bare path and the recipe
works. The failure is invisible in CI (never run) and intermittent locally
(depends on build residue) — issue 0828's shape exactly.

## Why the nightly does not cover it

`nightly.yml` has an `esp32` path filter and an esp32 lane module, so ESP32 looks
covered. It is — for the MATRIX, which builds fixtures through
`build-test-fixtures` and tests through the resolver. Neither route calls this
recipe. "The platform is covered" and "this recipe is covered" are different
claims, and only the first is true.

## Fix

Run it. It needs no ROS, no SDK beyond the esp32 toolchain the lane already
provisions, and it is a build, so it belongs wherever the ESP32 compile work
already happens — the nightly esp32 module, gated on the same path filter.

Assert the artifacts, not the exit code: the recipe's own history is of exiting
0 while producing nothing (issue #181, which is why the "ERROR: … is missing"
guard exists). The check is that `build/esp32-qemu/esp32-qemu-{talker,listener}
.bin` exist and are non-empty afterwards.

## Direction: this recipe should move onto RFC-0062 provisioning

Adding a lane is the interim step, not the destination. The recipe currently
hand-rolls its own dependency resolution — six `command -v` probes across
`just/esp32.just` for `espflash` and `qemu-system-riscv32`, each with its own
message and its own opinion about whether a miss is fatal (issue 0486 already
had to convert one from a warn-and-continue into a failure, precisely because a
skip read as green).

RFC-0062 is the SSoT for exactly this: `[prereq.*]`, one declaration namespace
over four providers, rosdep no longer consulted (phase-327 / phase-398 landed
it; phase-404 carries amendment 2, where the provider is chosen by what the tool
DOES and every resolution reports its provider). `[tool.esp32-qemu]` already
declares its build deps there — issue 1038 wired that up — so the recipe is the
part that has not moved.

Doing so replaces the six probes with a declaration, and gives the lane a
provisioning step that fails with a reason instead of a recipe that decides for
itself what to do about a missing tool. Sequence it after the lane exists: the
lane is what proves the move did not break the recipe, and adding a lane to an
unprovisioned recipe is how a "skip" becomes the CI answer.

## The general shape, worth stating once

A user-facing recipe whose output is also produced by a second, test-internal
path has no coverage from those tests, however green they are. The test proves
the SECOND path. Any recipe the book or `just --list` offers a human should have
one lane that invokes it the way a human would — otherwise its correctness is
asserted only by the last person who happened to run it.


---

# RETRACTED (2026-09-05). The premise is false.

`just esp32 build-qemu` **is** run by a lane, and that lane **did** report issue
1025, loudly, every night.

## What I got wrong, and how

`just/esp32.just:241` reads:

```
build-fixtures: build-qemu build-logging-smoke
```

and `build-all: build build-examples build-fixtures`, which is what
`nightly.yml`'s `Build (${{ matrix.plat }})` step runs on the schedule path. So
the chain `build-all -> build-fixtures -> build-qemu` has always existed.

I missed it because the grep I based this issue on ended in `| head -10`, and
that line was the eleventh. Everything downstream followed from a truncated
command whose truncation I never checked — the same shape as the
`nm ... 2>/dev/null | grep -c` probe earlier in this same investigation, where a
command that could not answer was read as an answer.

## The claim that should have caught it

I wrote that 1025 "stayed that way through many green CI runs". I did not look at
a single run. They were not green. From the 2026-09-04 scheduled nightly, job
`esp32`, step `Build (esp32)`:

```
ERROR: /__w/nano-ros/nano-ros/build/cargo-fixtures/qemu-esp32-baremetal/
       riscv32imc-unknown-none-elf/nros-relwithdebinfo/esp32_qemu_talker is
       missing, and nothing narrowed this build.
error: recipe `build-qemu` failed with exit code 1
```

That is issue 1025, named exactly, by the lane this issue says does not exist.
The workflow even carries a comment saying so: *"NOT the cause of this cell's RED
— that is issue 1025 (#303)"*. The evidence was in the repo I was editing.

## What survives

One change, kept and landed separately: `build-qemu` now asserts its OUTPUT
rather than espflash's exit code. That stands on its own — the recipe's history
is of exiting 0 while producing nothing (issue 0181), and the existing guard
covers only a missing INPUT.

## What is actually worth pursuing

The esp32 cell is red across every run in the scanned window (`just
nightly-triage`: *"red across all 3 scanned run(s)"*). That is the real
signal-capacity problem CLAUDE.md describes — a uniformly red lane cannot
report a NEW regression, because the new one looks like yesterday's. With #303
merged, the next nightly should show whether anything else is behind it; the log
also carries `nros setup --tool esp32-qemu: needs 3 system package(s) this host
is missing: libglib2-dev, libpixman-dev, libgcrypt-dev`, whose `[prereq.*]`
declarations are CORRECT (`apt = ["libglib2.0-dev"]` etc.), so that is a
provisioning question and not a naming one.

File that against the measured state after a green-or-not nightly, rather than
inheriting this issue's guesses.
