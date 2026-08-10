---
id: 486
title: "`espflash` is not provisioned, so a fully-provisioned host still cannot
  produce an ESP32 flash image — and the step says WARNING, not FAIL"
status: resolved
resolved_in: phase-340
type: bug
area: esp32
related: [issue-0399, issue-0368, issue-0181, phase-340]
---

## Symptom

On a host that has run `just esp32 setup` to completion, `just esp32
build-fixtures` builds every ELF and then produces no flash images:

```
Creating flash images...
  WARNING: espflash not found, skipping flash image for talker
  Install with: cargo install espflash
  WARNING: espflash not found, skipping flash image for listener
Creating logging-smoke-esp32-qemu flash image...
  WARNING: espflash not found, skipping flash image
ESP32 fixtures built.
```

Exit code 0. The lane reports success, and `build/esp32-qemu/` is empty.

## Why the setup does not cover it

`just esp32 setup` provisions three things and espflash is not among them:

| what | how |
| --- | --- |
| riscv32 target | rustup (board is `build-std`, no toolchain package) |
| `riscv-none-elf-gcc` | `[board.qemu-esp32-baremetal].packages`, added by issue 0399 |
| Espressif QEMU fork | `[tool.esp32-qemu]`, best-effort (`e2e` gates on it, build does not) |

The only instruction for espflash is prose — `docs/guides/esp32-setup.md:43`,
`cargo install espflash --locked` — plus the recipe's own warning text. Nothing
in `nros-sdk-index.toml` mentions it, so `nros setup qemu-esp32-baremetal`
cannot install it and `nros doctor` cannot report it missing.

## This is issue 0399's shape, one dependency over

0399 added `riscv-none-elf-gcc` to that board's `packages` with the reasoning
that declaring it "provisions the compiler zpico-build's detection now looks
for, instead of leaving a documented-provisioned host unable to build the
example". espflash is the identical claim about the **pack** step rather than
the compile step: the host followed the documented setup and still cannot
produce the artifact the lane exists to produce.

Issue 0368 already observed the absence in passing — "seven modules fail on
undeclared prereqs … (no espflash/target)" — but recorded it as a symptom of a
clean-host sweep rather than as a missing index entry.

## The second half: WARNING is the wrong verdict once it is provisioned

Today the warning is honest — nothing promised espflash, so skipping is correct
and issue 0181 deliberately made it a skip rather than a hard error.

**Once the index declares it, that inverts.** An absent espflash becomes a
provisioning failure, and a step that warns-and-continues turns a broken host
into a green lane with no artifacts — the "green while running nothing" class
this repo keeps paying for (0481's silent readiness timeouts, 0483's sixteen
permanently-skipping tests, 0445's absorbing STALE verdict). 0181's own summary
names the ancestor: "espflash opened a nonexistent ELF and the lane passed".

So the fix has two halves and the second is not optional:

1. Declare `[tool.espflash]` + add it to `[board.qemu-esp32-baremetal].packages`.
2. Change the skip to a FAILURE when espflash is absent, keeping a skip only for
   the reason issue 0439 established — a lane that narrowed this row out of the
   build, where there is legitimately no ELF to pack.

Those are distinguishable: 0439's skip is keyed on `NROS_FIXTURE_COORDS` being
set, not on the tool being missing.

## Pinning

Every `[tool.*]` in the index carries an exact `ref` and an `upstream` field.
A bare `cargo install espflash` resolves against crates.io at whatever version
is current on the day, which is the drift the pinning convention and the
project-wide `--locked` shim exist to prevent. `[tool.sccache]` and
`[tool.play_launch_parser]` are the precedents for a cargo-installed tool:
`install = "cargo install --path <subdir> --root {prefix} --locked"`.

At `v4.5.0` (2026-07-09) the repo's crate layout is `cargo-espflash/`,
`espflash/`, `xtask/` — the binary wanted here is the `espflash/` subdir.

## Fix, landed 2026-08-10

Three parts, because provisioning the tool turned out not to be enough:

1. **`[tool.espflash]`** in `nros-sdk-index.toml`, pinned to `v4.5.0` and
   source-built like `[tool.sccache]` / `[tool.play_launch_parser]`:
   `cargo install --path espflash --root {prefix} --locked`. The binary is the
   `espflash/` subdir of the workspace (`cargo-espflash/` is the separate
   cargo-subcommand front end). Added to
   `[board.qemu-esp32-baremetal].packages` beside `riscv-none-elf-gcc`.

2. **`activate.sh` puts it on PATH.** `nros setup --tool espflash` succeeded and
   the pack step *still* skipped: the store bin dir was never added, because
   that block whitelists tool basenames (gcc dirs, `genromfs`, `sccache`,
   `zenohd`) to keep the `build/<tool>` convention intact for qemu. `espflash`
   joins the whitelist for the same reason `genromfs` is on it — the caller
   invokes it by BARE NAME. **Provisioned is not the same as reachable**, and
   only running the recipe showed the difference.

3. **A missing espflash is now FATAL**, at both sites (`build-qemu` and
   `build-logging-smoke`). The `NROS_FIXTURE_COORDS` skip for a lane-narrowed
   row (issue 0439) is untouched — it keys on the lane signal, not on the tool.

## Verified

```
Creating flash images...
  build/esp32-qemu/esp32-qemu-talker.bin
  build/esp32-qemu/esp32-qemu-listener.bin
Creating logging-smoke-esp32-qemu flash image...
  …/fixtures-cargo/qemu-esp32-baremetal/…/logging-smoke-esp32-qemu.bin
```

All three images produced, 4.2 MB each. This also closes the "not verified"
caveat left by the `qemu-esp32-baremetal` migration commit, which could only
prove the ELF was found, not that the image packed.

Both directions checked, per the standing rule that a tripwire nobody has seen
fail is one nobody should trust: with the store bin dir stripped from PATH the
recipe now exits 1 with

```
ERROR: espflash not found, so no flash image can be packed.
       It is declared in nros-sdk-index.toml and provisioned by
         just esp32 setup      (or: nros setup --tool espflash)
```

where it previously printed a warning and exited 0.

## No system dependency

`serialport v4.9.0` compiles here with no libudev dev package installed, so
`[tool.espflash]` carries no `system = [...]`. Probed by building it rather than
assumed, because an undeclared system dep is the RFC-0062 class that made
`[tool.sccache]` carry `--features vendored-openssl`.
