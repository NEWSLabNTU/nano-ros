---
id: 487
title: "`[system.*].check` allowed exactly ONE probe, so libgcrypt reads as
  missing on Arch and `nros setup` demands a package that is already installed"
status: resolved
resolved_in: phase-340
type: bug
area: provisioning
related: [issue-0486, issue-0368, phase-327, rfc-0062]
---

## Symptom

On a host with libgcrypt fully installed:

```
$ nros setup --tool esp32-qemu
Error: nros setup --tool esp32-qemu: needs 1 system package(s) this host is missing: libgcrypt-dev.
  Install with:  sudo pacman -S --needed libgcrypt
```

The package it names is present:

```
$ pkg-config --modversion libgcrypt
1.12.2
$ pacman -Qo /usr/include/gcrypt.h
/usr/include/gcrypt.h is owned by libgcrypt 1.12.2-1
```

`libgcrypt 1.12.2-1` is exactly what the index's `pacman = ["libgcrypt"]` maps
to. The dependency is satisfied and the build is blocked anyway.

## Cause

```toml
[system.libgcrypt-dev]
check = { cmd = "libgcrypt-config" }
```

`libgcrypt-config` is a hand-rolled config script that upstream deprecated in
favour of pkg-config. Arch's `libgcrypt` 1.12 no longer ships it —
`pacman -Ql libgcrypt | grep -c libgcrypt-config` returns **0** — while shipping
`libgcrypt.pc`. Ubuntu 22.04's libgcrypt 1.9, which is where these mappings were
verified (issue 0368's clean-host walk), ships the script and **no** `.pc` file.

So the two supported hosts ship different halves of the same dependency, and
either probe alone is a false negative on one of them.

The schema made that unfixable:

```rust
if check.field_count() != 1 {
    bail!("[system.{key}].check must set exactly one of cmd/sharedlib/pkg_config");
}
```

`run_probe` matched — each arm `return`ed, so only the first declared probe
could ever answer.

## Why a false negative here is worse than a false positive

`nros setup` treats MISSING as fatal and prints a `sudo` command. So the failure
mode is: a correctly provisioned host is told to install, with elevated
privileges, a package it already has. A user who follows that advice gets a
no-op; a user who doubts it has no way to proceed, because there is no
`--skip-system-check`. It converts a working host into a blocked one, which no
amount of "best effort" elsewhere recovers from — `just esp32 setup` calls this
tool best-effort, but a direct `nros setup --tool esp32-qemu` is the documented
remedy the emulator tests' own skip message prints.

## Fix

**Probes are now OR-ed, and `check` takes AT LEAST one rather than exactly one.**

```rust
// PRESENT if any declared probe finds it; MISSING only if at least one probe
// could answer and none did; UNKNOWN if none was answerable on this host.
```

The `Unknown` distinction is preserved and load-bearing: a `sharedlib` probe off
Linux, or a `pkg_config` probe with no pkg-config binary, must not count as
evidence of absence. Only a probe that ran and said no sets `Missing`.

```toml
check = { cmd = "libgcrypt-config", pkg_config = "libgcrypt" }
```

## Verified

* `nros setup --system --check` no longer lists `libgcrypt-dev`; it reports
  `19 present, 3 missing, 2 unprobed`.
* The negative direction still works — `gcc-riscv64-unknown-elf`, `genromfs` and
  `kconfig-frontends` are genuinely absent and still report MISSING, so OR-ing
  did not turn the gate green across the board. That check matters more than the
  positive one: a permissive probe that says PRESENT for everything looks
  identical to a fixed one until something breaks at build time.
* `nros setup --tool esp32-qemu` proceeds past the system gate.

## The shape

This is the generic-tool rule from `packages/cli/CLAUDE.md` — "`nros` must not
learn the nano-ros directory layout; fixes are index edits, not CLI special
cases" — meeting its exception. The index could not express "this dependency has
two valid existence tests", so the fix had to be in the CLI. The alternative
considered and rejected was switching the single probe to `pkg_config`, which
just moves the false negative from Arch to Ubuntu 22.04.

The single-probe rule was a simplifying assumption that every dependency has one
right existence test. It held until a dependency shipped its two probes to two
different distros.
