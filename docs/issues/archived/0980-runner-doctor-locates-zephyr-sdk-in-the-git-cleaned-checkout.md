---
id: 980
title: "`runner-doctor` locates the Zephyr SDK in the job checkout, which
  `actions/checkout` git-cleans — so it fails every self-hosted runner whose SDK
  is where it has to be"
status: resolved
type: bug
area: ci
severity: high
found: 2026-09-01
related: [issue-0975, issue-0654, issue-0833, phase-395, phase-405]
---

## Symptom

Every `merge_group` run of the `queue` workflow goes red in `L3 (cross build +
link)`, before any build starts, at the first step:

```
=== nros-sdk-zephyr ===
  [OK] west: West version: v1.5.0
  [OK] Zephyr workspace: /mnt/evo/<user>/nano-ros/zephyr-workspace
  [MISSING] Zephyr SDK 0.16.8 is not at /home/<user>/actions-runner/_work/nano-ros/nano-ros/scripts/zephyr/sdk/zephyr-sdk-0.16.8
            this workspace's zephyr/SDK_VERSION demands exactly 0.16.8.
  [MISSING] no arm-zephyr-eabi-gcc under /home/<user>/actions-runner/_work/nano-ros/nano-ros/scripts/zephyr/sdk/zephyr-sdk-0.16.8
            the SDK directory exists but its toolchains are not unpacked —
            an interrupted install leaves exactly this state.
  [OK] Zephyr SDK 0.16.8 registered with cmake (/mnt/evo/<user>/nano-ros/scripts/zephyr/sdk/zephyr-sdk-0.16.8/cmake)
runner-doctor: FAIL — at least one label claims something this host does not have.
```

`just queue-triage` reports `queue` failing across **8 different pull
requests**, which is its INFRA verdict: the same check failing on unrelated
changes is not a property of any of them.

Read the four lines together. The function calls the SDK missing from one path
and then prints `[OK]` naming a *different* path where it is. One fact, two
derivations, and the disagreement is reported as a diagnosis rather than
noticed. The second `[MISSING]` is worse than useless — "the SDK directory
exists but its toolchains are not unpacked" was printed unconditionally, so the
commonest case (no SDK at that path at all) was reported as a half-unpacked
install, aiming the reader at the wrong remedy.

## Not a regression — the lane had never actually run

`queue` looks like it broke on 2026-08-31 at 18:57Z: green before, red after.
It is not a code regression. In every green run the job was **skipped**:

```
33423643227  success  2026-08-31T18:11:32Z   L3 (cross build + link) :: skipped
33427914059  failure  2026-08-31T18:57:50Z   L3 (cross build + link) :: failure
```

`l3` is gated on `vars.NROS_SELF_HOSTED_READY`. It flipped true at 18:57Z, the
job ran for the first time, and it failed at once — first on the workspace, then
(after the operator set `NROS_ZEPHYR_WORKSPACE`) on the SDK. The green history
is the interlock working, not the check passing. This is the same day, and the
same flip, as issue 0975.

## Root cause: the doctor's location is not the build's location

`runner-doctor.sh` derived the SDK from the checkout —
`<root>/scripts/zephyr/sdk/zephyr-sdk-<ver>`, where `scripts/zephyr/setup.sh`
puts it on a dev box — with `ZEPHYR_SDK_INSTALL_DIR` as the only override.

**That path cannot be right on a self-hosted runner.** `scripts/zephyr/sdk/` is
gitignored (`scripts/zephyr/.gitignore:3`), so is `/zephyr-workspace`
(`.gitignore:25`), and `actions/checkout@v4` defaults to `clean: true`, which is
`git clean -ffdx` — **`-x` removes ignored files**. Anything provisioned inside
the job checkout is deleted at the top of the next job. A runner's 9.2 GB SDK
has to live outside the checkout, and this operator's does.

Meanwhile the tree already documents where the build looks, in
`scripts/build/zephyr-toolchain.sh`:

> With `ZEPHYR_SDK_INSTALL_DIR` still unset the lookup takes the same `else()`
> search branch as before, and the in-tree SDK is found the way it always was —
> **through the CMake user package registry (`~/.cmake/packages/Zephyr-sdk/*`,
> written by `scripts/zephyr/setup.sh`)**, not through the hard-coded
> `/usr /opt $HOME` list, which never contained it.

So the registry is the normal answer for every SDK-toolchain board in this tree,
and `runner-doctor` was the only place resolving the SDK any other way. Worse,
its own registration check three blocks down was *already reading the registry*
— it had the true path in hand and printed it in an `[OK]` line.

The doctor's header names this exact failure mode as the thing it exists to
prevent:

> a doctor telling a working host it is broken is the failure mode a doctor
> exists to prevent (issue 0654)

and it committed it, because the remedy it offers (`ZEPHYR_SDK_INSTALL_DIR`) is
one `scripts/build/zephyr-toolchain.sh` explicitly tells you not to use for the
build.

## Fix

`_nros_runner_zephyr_sdk_path <want> <root>` is now the single resolver, and its
precedence mirrors `FindZephyr-sdk.cmake`:

1. `ZEPHYR_SDK_INSTALL_DIR` — both spellings (the SDK, or its parent);
2. the CMake user package registry, via
   `_nros_runner_zephyr_sdk_registry_path`;
3. `<root>/scripts/zephyr/sdk/zephyr-sdk-<ver>` — the dev-box default, kept as
   the fallback for a host with neither of the above.

Every sub-check now reads that one answer, and each `[OK]`/`[MISSING]` line
names the ORIGIN (`via cmake package registry`) so the next reader can see why
that path and not another.

Three smaller defects in the same function, all of the same "named is not
present" shape, went with it:

* the registration check grepped the registry file's TEXT and never checked that
  the directory it names exists, so a stale entry outliving its SDK reported
  `[OK]`. It now distinguishes registered-and-present from
  registered-and-gone, and says which.
* a registry entry whose directory is missing no longer shadows a live entry for
  the same version, whatever order the glob yields them in.
* the "interrupted install" remedy is printed only when the directory is
  actually there.

## Gate

`just check runner-doctor-sdk-resolution` → `runner-doctor.sh --self-test`, on
the fast line. Temp dirs only: no SDK, no cmake, no cargo, no network. Nine
cases — the three registry-resolution cases, the wrong-version case, the
dev-box default, both `ZEPHYR_SDK_INSTALL_DIR` spellings, and two end-to-end
runs of the whole `nros-sdk-zephyr` check against a host shaped like the runner
that failed.

Proven non-vacuous: restoring the old precedence fails exactly four cases and
passes the other five —

```
FAIL an SDK outside the checkout is found via the cmake registry
FAIL a live registry entry beats a stale one
FAIL a stale-only entry is reported, not silently discarded
ok   an entry for another SDK version is ignored
ok   with no env and no registry the checkout default is used
ok   ZEPHYR_SDK_INSTALL_DIR naming the SDK beats the registry
ok   ZEPHYR_SDK_INSTALL_DIR naming the parent appends the version
FAIL a runner with its SDK outside the checkout verifies clean
ok   a host with no SDK at all still fails
```

The last of those is the one that keeps the gate honest: the check must still be
able to go red.

## Verified

A replica of the runner's layout — workspace and SDK outside the checkout,
registry pointing at the SDK, `ZEPHYR_SDK_INSTALL_DIR` unset — reproduces the
production output line for line before the fix, and after it:

```
  [OK] Zephyr SDK 0.16.8 unpacked: .../elsewhere/zephyr-sdk-0.16.8 (via cmake package registry)
  [OK] arm-zephyr-eabi toolchain present (SDK is unpacked, not just downloaded)
  [OK] Zephyr SDK 0.16.8 registered with cmake (.../elsewhere/zephyr-sdk-0.16.8/cmake)
runner-doctor: OK — all 1 label(s) verified.
```

**What this does NOT prove.** The fix removes a red that was WRONG; it cannot
make an SDK exist. The runner's registry names
`/mnt/evo/<user>/nano-ros/scripts/zephyr/sdk/zephyr-sdk-0.16.8` — if that tree has
since been deleted or was never fully unpacked, `L3` stays red, now saying so
truthfully (`cmake's registry names <path>, but nothing is there`, or `no
arm-zephyr-eabi-gcc under <path>`). The next `queue` run is the measurement, and
nobody with access to that host has confirmed the SDK is intact.

## Sibling of the same class, left open

`_nros_runner_check_qemu` locates the patched QEMU at `<root>/build/qemu/bin/
qemu-system-arm`. `build/` is gitignored (`.gitignore:22`), so it is wiped by
the same `git clean -ffdx` on every self-hosted job. It does not currently
produce a false red, because it falls back to `qemu-system-arm` on PATH and
checks the >= 7.2 rule there — but a runner provisioned with only the
project-local build will be told it has no QEMU. It is deliberately NOT fixed
here: unlike the SDK, QEMU has no registry to consult, so the checkout-
independent location would have to be invented rather than adopted, and that is
a decision rather than a repair. It bites when the `nros-qemu` label is asserted
by a self-hosted lane — `nightly.yml` and `run-matrix.yml` both do.

## Acceptance

* [x] `runner-doctor` resolves the SDK the way the build does, and every
      sub-check reads one answer.
* [x] A gate that fails if the precedence is restored, and can still fail on a
      genuinely broken host.
* [ ] `queue`'s `L3` job green on the merge group — depends on the runner's
      SDK actually being intact, which this change cannot establish.
