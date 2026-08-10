---
id: 451
title: "Embedded SDK env vars are set only by the `just` recipes, so a bare cargo build fails in a way that reads like a code fault"
status: resolved
type: bug
area: build
related: [issue-0407, issue-0420, issue-0431, issue-0491, phase-338, phase-345]
---

> **RESOLVED 2026-08-10 by phase-345 W1 — with the CAUSE corrected.** This
> issue says the defaults "are set only by the `just` recipes". They are not:
> `activate.sh` sources `scripts/sdk-env.sh`, which evaluates the
> `just/sdk-env.just` SSoT. The delivery mechanism existed; its COVERAGE did
> not, because three separate lists decided which variables survived, and each
> disagreed with the SSoT differently — measured in a clean environment:
>
> | shell | before | after | what dropped them |
> |---|---|---|---|
> | bash | 14 / 23 | 23 / 23 | a hand-written 14-name array beside a 23-name SSoT — the nine omitted were exactly the first-party paths |
> | fish | 2 / 23 | 23 / 23 | `activate.fish` dumped `env` and imported only `NROS_*`, dropping every third-party SDK root |
> | zsh | 0 / 23 | 23 / 23 | bash-only `${!name}` → `bad substitution`, and a bash-only sourced-vs-executed test that sent a sourced zsh down the "print" branch |
>
> Fixed by deriving the list from the SSoT in all three places (and taking the
> values from ONE `just --evaluate` instead of one subprocess per variable).
> `check-activate-shells.sh` — which passed throughout, because it asserted
> COMPLETION rather than delivery — now also asserts that every SSoT variable is
> set, reads that list from the SSoT, and runs each probe under `env -i`: on a
> direnv host the probe otherwise inherited 22 of the 23 it was checking, and
> the first version of the assertion passed for exactly that reason.
>
> Acceptance: in a clean environment with only `source ./activate.sh`, a bare
> `cargo build` in `examples/qemu-arm-freertos/rust/talker` completes (exit 0).
>
> The related "loud panic" ask stands as written and was NOT needed: no panic
> was reachable at the default paths once coverage was complete.

## Symptom

Building any embedded example directly — the thing a user does after copying one
out (RFC-0026), and the thing an agent does when narrowing a failure — dies on a
missing environment variable, one at a time, with no indication that a whole set
is required:

```
FREERTOS_DIR not set. FreeRTOS kernel source. just setup-freertos; export FREERTOS_DIR=...
FREERTOS_PORT not set. FreeRTOS portable layer. Examples: export FREERTOS_PORT=GCC/ARM_CM3
LWIP_DIR not set. lwIP source. just setup-freertos; export LWIP_DIR=...
FREERTOS_CONFIG_DIR not set. Directory containing FreeRTOSConfig.h + lwipopts.h
```

Four rounds for FreeRTOS. Measured across the lanes:

| platform | vars needed by a direct build |
|---|---|
| qemu-arm-freertos | `FREERTOS_DIR`, `FREERTOS_PORT`, `LWIP_DIR`, `FREERTOS_CONFIG_DIR`, `NROS_LAN9118_LWIP_DIR` |
| threadx-linux / riscv64 | `THREADX_DIR`, `NETX_DIR`, `THREADX_CONFIG_DIR`, `NETX_CONFIG_DIR` |
| qemu-arm-nuttx | `NUTTX_DIR` (+ `NUTTX_APPS_DIR` for some paths) |

All of them have correct defaults in `just/sdk-env.just`, e.g.

```just
export FREERTOS_DIR := env("FREERTOS_DIR", justfile_directory() / "third-party/freertos/kernel")
export NUTTX_DIR    := env("NUTTX_DIR",    justfile_directory() / "third-party/nuttx/nuttx")
```

So the `just` recipes work and the same build by hand does not, even though the
SDKs are present at exactly those default paths.

## Why it matters more than an inconvenience

**The failure does not look like a missing variable.** It looks like a code
fault, and it costs the reader real time before they find the door:

* `zpico-sys/build.rs` panics deep in a dependency's build script;
* on NuttX a partially-configured build reaches the LINKER and emits
  `undefined reference to open / socket / ioctl / malloc` — indistinguishable
  from a genuine link regression, which is exactly what it was mistaken for
  during phase-338;
* CLAUDE.md's own pitfall index says `source ./activate.sh` is what makes builds
  work — and for these variables it is NOT, which makes the documented remedy
  misleading.

This is the same shape as issue 0420's real finding (NuttX cells SKIP silently
because `NUTTX_DIR` is unset) and issue 0407: a lane that works through one door
and fails confusingly through the other.

## Fix

Move the defaults to where both doors see them — `activate.sh` / `activate.fish`
/ `.envrc`, which CLAUDE.md already names as "the env/PATH SSoT" — with
`just/sdk-env.just` reading them rather than duplicating. Keep the loud panics;
they are good. They just should not be reachable when the SDK is sitting at the
default path.

If a variable must stay recipe-scoped, the panic should say so: "set by
`just <platform> …`; not exported by activate.sh" beats a bare "not set", which
implies the user forgot something they never had.
