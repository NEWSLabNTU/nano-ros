---
id: 1628
title: "`just ci tier1` runs `rust-rtos-link-check`, which needs the FreeRTOS
  kernel that `host-tests.yml` never checks out — both tier-1 arms now fail on a
  provisioning refusal that 1147 had been hiding"
status: open
type: bug
area: ci, freertos
severity: medium
found: 2026-10-02
related: [issue-1147, issue-1345, issue-1353, issue-1043]
---

## Measured

Both tier-1 arms, the same failure, on different events and different heads:

| run | event | head | job | step |
| --- | --- | --- | --- | --- |
| **36952778118** | push | `68aa44ef7` | **110669130575** | 15 `just ci tier1` |
| **36959688027** | schedule | `07e5832a4` | **110690404490** | 15 `just ci tier1` |

```
thread 'main' panicked at packages/rmw/zenoh/nros-zpico-build/src/runner.rs:2900:17:
FREERTOS_DIR=…/third-party/freertos/kernel: missing include (expected at
…/third-party/freertos/kernel/include). FreeRTOS kernel source.
<remedy>; export FREERTOS_DIR=$PWD/third-party/freertos/kernel
error: recipe `rust-rtos-link-check` failed with exit code 101
error: recipe `tier1` failed with exit code 101
```

`<remedy>` stands in for what the message actually printed, which was a recipe
name that **does not exist** — see the section below. Writing it verbatim here
makes `check-doc-recipe-refs` fail, which is how this was found.

**The build script is behaving correctly.** It refuses loudly, names the missing
directory, and names the remedy. The defect is that the lane asks for a source it
does not provide: `git grep` over `.github/workflows/host-tests.yml` finds no
`submodules:`, no `setup-freertos`, and no freertos provisioning of any kind,
while `third-party/freertos/kernel` is a declared submodule in `.gitmodules`.

## This is newly VISIBLE, not newly broken

Earlier tier-1 reds in this window never reached `rust-rtos-link-check`:

- 36938229060 and 36950039916 died at `===== FAIL (mem-report, rc=1, …)` — issue
  **1147**'s drift tripwire, in under three seconds.

PR #1541 fixed 1147 and merged at 01:21:52. Both runs above have heads created
after that (`68aa44ef7` at 01:49, `07e5832a4` at 03:19), **`mem-report` does not
appear as a FAIL in either log**, and the lane therefore ran on to the next wall.
This is CLAUDE.md's own warning that one fix unmasks the next, and it means the
lane has probably been unable to pass this step for as long as it has existed —
nothing had got far enough to find out.

## What this is NOT

- **Not 1353.** Both jobs report `freed 32533 MB; 48645316 KB free` — 46.4 G free
  — and neither log contains `No space left` or `Free space left`. The disk was
  not the constraint.
- **Not 1147.** Fixed, present in both heads, and `mem-report` passed.
- **Not 1345.** Different lane and different gates; that one is
  `capability-conditionals` + `xrce-vendored-versions` on the push `gate`.
- Not a flake: two runs, two events, two heads, byte-identical panic site
  (`runner.rs:2900:17`).

## The class it belongs to

This is the `check-lane-contracts` rule one lane over — *a gate in an
affordability tier may only resolve artifacts the job itself provides*. 1345 is
the same shape on the push `gate` (two gates needing vendored trees the event
does not check out), and this issue is its tier-1 twin. Worth noting because the
remedy choice is the same choice, and the repo has already made it once.

## What would close this

Three options, and the trade-off is real rather than obvious:

1. **Provision it in the lane.** `third-party/freertos/kernel` is one submodule
   checkout; the lane already spends an hour, so the cost is trivial. This makes
   `rust-rtos-link-check` a real verdict on the tier-1 boards, which is what the
   tier promises.
2. **Move `rust-rtos-link-check` out of tier 1** to a lane that provisions RTOS
   sources. Cheapest, and it narrows what tier 1 claims — which should then be
   said out loud, because `board-support.toml` grants tier 1 the FreeRTOS boards.
3. **Report NOT VERIFIED** through the `nros_check_skip` ledger when the source
   is absent, the three-outcome treatment issue **1043** prescribes and
   `check-submodule-pins` already implements. This keeps the check honest on
   provisioned lanes and stops it failing closed on unprovisioned ones — but a
   skip here must not be able to pass for a verdict, which is the trap 1345's
   own remedy discussion names.

Acceptance either way: a `host-tests` run that reaches `just ci tier1`'s later
steps, so the lane's verdict is about the tests rather than about its own
environment.

## Found on the way: the remedy text named a recipe that does not exist

`check-doc-recipe-refs` rejected the first version of this issue:

```
check-doc-recipe-refs: 1 document(s) name a recipe that does not exist:
  docs/issues/1628-….md: just setup-freertos;
  A reader copies these. Name the recipe that exists, or drop the line.
```

The gate is right, and the stale name was not mine — I had quoted it from the
panic. **Ten live sites told a reader to run a recipe that was never defined**,
across three kinds of file:

| file | sites | what a reader sees |
| --- | --- | --- |
| `just/freertos.just` | 4 | `ERROR: FreeRTOS not found at … Run: <stale>` |
| `packages/platform/nros-platform-freertos/nros-platform.toml` | 2 | the `help` for `FREERTOS_DIR` and `LWIP_DIR` — what the panic above prints |
| `packages/platform/nros-platform-nuttx/nros-platform.toml` | 1 | the `help` for `NUTTX_DIR` |
| `packages/testing/nros-tests/tests/freertos_{posix,qemu}.rs` | 3 | the skip message when the env is unset |

The working spellings are `just freertos setup` and `just nuttx setup` —
`just setup-platform <platform>` is their documented alias and is literally
`@just "{{platform}}" setup`. All ten are corrected with this issue.

### The gate's reach is narrower than its rule (issue 0196's shape)

`check-doc-recipe-refs` says "a reader copies these", and reads `docs/` only. The
remedy text a reader actually hits — a build-script panic, a `just` recipe's
error echo, a test's skip message — is in `just/`, `packages/**/*.toml` and
`packages/**/*.rs`, where the gate cannot see it. That is why ten sites survived
while the gate guarded the documentation. Widening it to those three file kinds
would be a small change and would have caught all ten.

### FOUR threadx sites are deliberately left alone

The threadx equivalent (`setup-threadx`, written without the `just` prefix here for the same reason as above) is also undefined — `packages/platform/nros-platform-threadx/nros-platform.toml`
lines 71 and 73, and `packages/testing/nros-tests/tests/threadx_riscv64_qemu.rs`
lines 54 and 59 — but unlike freertos and nuttx, **`just --list threadx` has no
`setup` recipe at all**, so there is no correct spelling to substitute and
guessing one would just move the lie. `just/check/codegen.just:245` already
carries a comment noting this exact case, so it has been seen once before and
recorded rather than fixed. Deciding what provisions ThreadX is a separate change
with an owner who knows the answer.
