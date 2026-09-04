---
id: 1025
title: "ESP32 flash images can never be built: the packer asks for the group dir with the row's env stripped, so it looks in a directory the build stopped using"
status: resolved
type: bug
area: build, testing
severity: high
found: 2026-09-04
resolved_in: "fix/1025-esp32-flash-image-dir (2026-09-04)"
related: [0968, 0535, 0439, phase-340, 0393, 0181, 0945]
---

# The producer wrote to a hashed group dir; the consumer asked for the unhashed one

## Mechanism

`nros_fixture_group_slug` keys the shared cargo target dir on a row's VARIANT —
a `cksum` over (cargo args, sorted env). No variant gives the bare platform
name; a variant gives `<platform>-<cksum>`.

phase-340 item 7 had already made the group FORMULA single
(`nros_fixture_row_artifact_dir`). Its INPUTS were still derived twice. The
packer called it as

```sh
nros_fixture_row_artifact_dir "examples/…/$ex" qemu-esp32-baremetal "" ""
```

— platform from the call site, args and env as two empty literals — while the
producer (`fixtures-build.sh` → `nros_fixture_target_dir_flag`) passed the
ROW's. Two of three constants supply a different answer. Both sides agreed for
as long as the esp32 rows carried no variant, and diverged when `41a7d8de7`
added `env = { ZPICO_MAX_QUERYABLES = "2" }` to all three of them.

## Reproduced, then fixed, both by BUILD

`just esp32 build-qemu` on a clean worktree at `91241878b`, before the fix:

```
Creating flash images...
ERROR: …/build/cargo-fixtures/qemu-esp32-baremetal/riscv32imc-unknown-none-elf/
       nros-relwithdebinfo/esp32_qemu_talker is missing, and nothing narrowed this build.
rc=1
```

and the same run's output, one directory over:

```
build/cargo-fixtures/            -> qemu-esp32-baremetal-4118800323   (the ONLY entry)
  …/nros-relwithdebinfo/esp32_qemu_talker      9137428 bytes
  …/nros-relwithdebinfo/esp32_qemu_listener    9126712 bytes
build/cargo-fixtures/qemu-esp32-baremetal      No such file or directory
```

After the fix, same tree, cargo untouched:

```
just esp32 build-qemu        rc=0  ->  build/esp32-qemu/esp32-qemu-talker.bin   (4 MiB)
                                       build/esp32-qemu/esp32-qemu-listener.bin (4 MiB)
just esp32 build-logging-smoke rc=0 ->  …-4118800323/…/logging-smoke-esp32-qemu.bin
```

## The fix

`nros_fixture_row_artifact_dir_by_id <row-id>` in
`scripts/build/fixtures-target-dir.sh`: it reads (platform, dir, env, args) from
`fixtures-manifest.py list --with-platform --builder cargo --id`, which is the
same record `fixtures-build.sh` builds from, and hands them to the existing
helper. Same table, same row, same three fields — the two cannot disagree.
`--builder cargo` is load-bearing: `list` emits a different record SHAPE for a
cmake row, so restricting the query makes a cmake id a loud failure rather than
a four-field misparse.

Both esp32 pack sites converted (`build-qemu`, `build-logging-smoke`), and
`build-qemu`'s two independent id lists folded into one.

## The class, and the gate

`check-fixture-artifact-dir-keys` (fast lane). Two rules:

1. An UNPAIRED `nros_fixture_row_artifact_dir` call whose key is spelled in
   literals must name a group some manifest row builds into. "Unpaired" is the
   mechanism stated as a predicate: a recipe that also calls
   `nros_fixture_target_dir_flag` with the SAME literal triple in the same body
   builds what it reads and cannot look where it did not write (the interactive
   `_run-qemu` recipes are that shape); a call with no such partner is consuming
   the manifest-driven fixture build, whose key comes from the row.
2. A literal `cargo-fixtures/<slug>` path must name a slug the manifest
   produces. Four survive, in `just/qemu-baremetal.just` and
   `scripts/check-weak-symbols-image.sh`, correct today only because
   `qemu-arm-baremetal` and `freertos` have one group each.

Mutation-checked in both directions, against the real defect and not only a
synthetic one: restoring the pre-fix call sites makes it name both, with the
right slugs; pointing a literal at `cargo-fixtures/qemu-arm-baremetal-1234`
makes it name that. It also runs a synthetic negative control on every
invocation (`check-gate-selftests`), against a synthetic ROW TABLE — driving the
control off a real variant row was tried and is wrong, because
`examples/native/rust/talker` carries a plain `linux` row beside eight variants
and a hand-spelled `linux "" ""` there is CORRECT, so the control silently
disarms.

The issue asked for acceptance by BUILD and not by gate, and that is what
happened: the build above is the acceptance and the gate is additional.

## Measured, not inferred: the test-side locators needed no change

`build_esp32_qemu_example` and `build_test_fixture_at_profile` both hand-spell
`<leaf>/target/<triple>/<profile>/<bin>`, which nothing writes. They are correct
anyway, because `require_prebuilt_binary` redirects through
`fixtures::groups::resolved` at the chokepoint. Confirmed by running the real
resolver rather than by reading it — a temporary integration test resolved all
three authored leaf paths and each landed on the group dir, `exists=true`.

## NOT fixed, deliberately

`just nuttx _run-qemu` builds AND reads with `nuttx "" ""`, so it is
self-consistent and nothing is broken — but every
`examples/qemu-arm-nuttx/rust/*` manifest row carries
`NROS_LOCATOR=tcp/10.0.2.2:82x0 NROS_DOMAIN_ID=0`, so the recipe compiles a
THIRD population of those crates into a group the fixture lane never uses. That
is a phase-340 P2 duplicate-build defect, not a broken read. Fixing it means
also delivering the row's env to the build (the locator is baked at compile
time), i.e. a behaviour change on an interactive path that needs a NuttX
toolchain and QEMU to test — neither available in the session that fixed this.
Recorded here and in the gate's docstring rather than folded in unmeasured.

## Bearing on issue 0968

Still NOT established. 0968's five esp32 tests all skip on a host without the
Espressif QEMU fork (`qemu-system-riscv32 -machine help` has no `esp32c3`
here), so this fix removes a certain blocker without demonstrating it was
THEIR blocker. Confirming that needs a host with the fork.
