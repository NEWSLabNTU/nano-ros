---
id: 1916
title: "`duplicate_deploys_that_agree_are_not_ambiguous` passes on a host with no FreeRTOS in its SDK store and fails on a provisioned runner: the two deploys resolve `NROS_SDK_FREERTOS_KERNEL` differently"
status: resolved
type: bug
area: [cli, testing]
severity: medium
found: 2026-10-15
resolved: 2026-10-15
related: [0755, 0934, 1788, 1915]
---

## Measured

Run-matrix **38663782341** (workflow_dispatch, main `9e9f24392`), job
**116199403780** `tier 2 (1-wise matrix)` on a self-hosted runner. In its
preflight, `check-build` reported `2 of 26 gate(s) FAILED`. One was
`template-copy-out` (issue 1914); the other is this one:

```
===== FAIL (cli-tests, rc=101, 94004ms) =====
---- cmd::board_facts::tests::duplicate_deploys_that_agree_are_not_ambiguous stdout ----
panicked at nros-cli-core/src/cmd/board_facts.rs:1138:10:
agreeing duplicates resolve: deploys `freertos` and `mps2-an385-freertos` resolve DIFFERENTLY (NROS_SDK_FREERTOS_KERNEL); pass --deploy or --board to say which one this build is
test result: FAILED. 1690 passed; 1 failed
```

The same test **passes** on `origin/main` on `newslab-server-241`, a host whose
SDK store holds no FreeRTOS (`~/.nros/sdk/freertos*` matches nothing):

```
test cmd::board_facts::tests::duplicate_deploys_that_agree_are_not_ambiguous ... ok
```

It also passes in every merge-gating lane, which runs on clean GitHub-hosted
images.

## Reading

The test fakes `FREERTOS_DIR` and `LWIP_DIR` through its `env` closure and
nothing else. `NROS_SDK_FREERTOS_KERNEL` is one of the facts compared between
the two deploys. On a runner whose store has the FreeRTOS kernel provisioned,
at least one of the two deploys apparently resolves it from the **store**
instead of from the injected environment, so the two disagree. The test then
asserts the agreement it was written to prove, against a host fact it did not
control.

That makes this a hermeticity defect in the test **or** a real asymmetry in
how the two deploy spellings resolve a store-backed fact. It is not yet known
which:

- If both deploys **should** read the store identically, the asymmetry is a
  product bug the test just caught. The error message ("pass --deploy or
  --board") would then hit a real user with a provisioned store.
- If the store read is legitimate and only the test is under-specified, the
  fix is to make `resolve`'s store root injectable (like `env`) and pin it to
  an empty temp dir in the test.

## What this is NOT

- **Not 1788** (cli-tests "No ROS packages found" on the new runners). A
  different test, a different message.
- **Not 1914.** It is the other gate in the same preflight failure.
- **Not caught by any merge lane**, because those images have no store.

## What would close this

1. Find where `NROS_SDK_FREERTOS_KERNEL` is resolved for each deploy, and
   whether the two spellings take different paths to the store.
2. Either fix the asymmetry (product), or inject the store root and pin it in
   the test (hermeticity). Then add the other half as a regression: the test
   run with a populated temp store must still see agreement.
3. Acceptance: the test passes on a host whose store has the FreeRTOS kernel
   provisioned.

## Resolution — the same failure as issue 1915, fixed there

Issue 1915 was filed for this exact failure, at the same time and from the
same run (38663782341). It was closed by `3f563f9ae8`. The answer to "which
of the two readings" is **neither**: the test is hermetic enough, and the
product resolves both spellings the same way.

`doctor::tests::license_gate_board_filter_arm_fvp_warns_only` set the
process-wide `$HOME` on a parallel test thread. The store rung derives from
`$HOME`. So a `resolve()` that ran across that change saw the runner's
FreeRTOS kernel in the store for one deploy and the checkout for the other.
After `3f563f9ae8`, `nros-cli-core`'s lib tests set no `HOME`/`NROS_HOME`.
Sweep: `git grep -nE 'set_var\("(HOME|NROS_HOME)"' -- packages/cli/nros-cli-core`
finds no hit.

Measured in the box tree, current main: with a store that holds
`sources/freertos-kernel/10.6.2+0adc196d` (via `NROS_HOME`) and
`NROS_SDK_FREERTOS_KERNEL` set in the environment, `board_facts` + `doctor`
pass 3/3. The race itself did not reproduce on demand. Old `doctor.rs` with
a `$HOME` store went 0/200 on the two tests alone. The window is narrow, and
the runner hit it under the full 1690-test parallel run.
