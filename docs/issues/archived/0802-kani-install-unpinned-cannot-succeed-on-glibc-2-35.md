---
id: 802
title: "`just verification kani` installs an unpinned kani-verifier, so the proof
  tooling is whatever crates.io published today — and today's `kani-driver` needs
  GLIBC_2.39, which no Ubuntu 22.04 host has"
status: resolved
type: bug
area: verification, ci
related: [issue-0500, phase-382]
---

## Problem

`just/verification.just:45`:

```sh
cargo install --locked kani-verifier && cargo kani setup
```

No version. **`--locked` reads like a pin and is not one** — it pins the crate's
own dependency graph, not which version of `kani-verifier` you get. So the
version of the bounded model checker that proves this repo's harnesses is
whatever crates.io published most recently at the moment someone ran the recipe.

Today that is **0.67.0**, and it does not run on this host:

```
$ ~/.kani/kani-0.67.0/bin/kani-driver --version
kani-driver: /lib/aarch64-linux-gnu/libc.so.6: version `GLIBC_2.39' not found
    (required by kani-driver)
```

Host is aarch64 Ubuntu 22.04, **glibc 2.35**. GLIBC_2.39 is Ubuntu 24.04.

## The precise fact, because the obvious summary is wrong

"The 0.67.0 bundle needs glibc 2.39" is NOT true, and believing it sends you
looking for a whole-toolchain problem. Scanning every executable in the bundle:

| binary | max GLIBC symbol required |
| --- | --- |
| `kani-compiler` | 2.34 |
| `libkani_macros.so` | 2.34 |
| every other `.so` | 2.34 |
| **`kani-driver`** | **2.39** |

**One binary** is over the line — and it happens to be the entry point
`cargo-kani` execs, so the failure looks total. The compiler that does the actual
work would have run fine here.

Reproduce:

```sh
for f in $(find ~/.kani/kani-<ver> -type f -executable); do
  printf '%s  %s\n' \
    "$(objdump -T "$f" 2>/dev/null | grep -o 'GLIBC_2\.[0-9]*' | sort -V | tail -1)" \
    "$(basename "$f")"
done | sort -V | tail
```

## Why it matters

**Verification tooling must be pinned, and everything else here already is.**
`scripts/gen-abi-bindings.sh` pins bindgen-cli to 0.72.1. Submodules pin
commits. `Cargo.lock` moves only when a dev means it, enforced by a PATH shim
that injects `--locked` project-wide. The one tool whose output is a *proof* is
the one installed from a floating version — so "the harnesses verify" is a claim
about an unrecorded version of Kani, and two developers can get different
answers with no way to tell from the repo.

**The failure mode is silent on a host that already has a bad install.** The
recipe's own guard:

```sh
if command -v cargo-kani &>/dev/null && [ -d "$HOME/.kani" ]; then
    kani_ver=$(basename "$(ls -d "$HOME"/.kani/kani-* | ... | head -1)")
    echo "kani-verifier already installed ($kani_ver)"
```

It reports whichever directory sorts first and never checks it is the version
this repo wants, or that it *runs*. `just verification doctor` does the same and
prints `[OK]`. The SDK store accumulates exactly like issue 0500's Corrosion
prefixes: `~/.kani/` here holds both `kani-0.62.0` and a broken `kani-0.67.0`,
and nothing in the repo expresses which one is correct.

## Evidence

Found during phase-382 W2', where the parameter-store proofs are in scope. The
work could only be verified by manually downgrading to **kani-verifier 0.62.0**
(`cargo kani setup`, CBMC 6.6.0), which runs on glibc 2.35 and returns
`nros-params` 17/17 and `nros-ghost-types` 28/28 VERIFICATION:- SUCCESSFUL.

So the proofs are fine. The recipe that is supposed to obtain the prover is not.

## Resolution (2026-08-26)

All three directions below are implemented in `just/verification.just`:

* **`KANI_VERSION := "0.62.0"`**, with the glibc floor and the reason for a low
  pin in a comment beside it, plus the `objdump` one-liner to re-check when
  someone wants to raise it.
* **The install recipe asks the DRIVER, not the filesystem.** `cargo kani
  --version` replaces the `ls ~/.kani/kani-*` guess, so a stale bundle no longer
  shadows the pin, and a version that installs but cannot execute reports
  nothing and is treated as absent. A mismatch REPLACES rather than reports.
  After installing it re-reads the version and fails loudly if it disagreed,
  naming glibc as the thing to check.
* **`doctor` fails instead of printing `[OK]`** on a wrong version (`[WRONG]`)
  or an unrunnable one (`[BROKEN]`).

Mutation-checked: pointing the pin at a version that is not installed turns the
`[OK]` into `[WRONG] kani-verifier 0.62.0, this repo pins 0.67.0` and exits 1, so
the guard is live rather than decorative.

Not fixed, because it is not ours: 0.67.0's `kani-driver` is over-linked
upstream. If a future Kani needs a newer glibc than the hosts we support, the
pin is the lever.

## Direction (as filed)

1. **Pin the version** in `just/verification.just`, as a variable so there is one
   place to move it, and say in a comment why the pin is low rather than latest.
2. **Verify the installed version matches the pin, and that it RUNS** — a
   directory existing under `~/.kani` is not evidence, per 0500. `doctor` should
   fail on a mismatch instead of printing `[OK]`.
3. Leave the floor documented: 0.62.0 is chosen because its `kani-driver` needs
   only glibc 2.34, so it covers Ubuntu 22.04 and 24.04 alike. A newer pin is
   fine when 22.04 stops mattering — that is a decision, not a default.
