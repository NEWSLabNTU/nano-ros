---
id: 1935
title: "`nros setup --tool X --source Y` installs the tool, never provisions the sources, and exits 0"
status: resolved
type: bug
area: [cli, setup, sdk]
severity: medium
found: 2026-10-17
related: [1038, 1453]
---

## Measured

`origin/main` `8f7e56feeb`, in-tree CLI built from it, a scratch store
(`NROS_HOME=<scratch>/psetup-home-before`, empty):

```
$ nros setup --tool arm-none-eabi-gcc --source freertos-kernel --source lwip
nros setup --tool arm-none-eabi-gcc: prebuilt 13.2-nros5 (dist linux-x86_64) → <scratch>/sdk/arm-none-eabi-gcc/13.2-nros5
    … fetched arm-none-eabi-gcc-linux-x86_64.tar.zst: 201.6 MB in 3m02s (1.10 MB/s)
nros setup --tool: locked in <checkout>/nros-sdk.lock
rc=0
```

No `nros setup --source …` line was printed, and the store holds only
`sdk/`: `[source.freertos-kernel]` and `[source.lwip]` are both
`location = "store"`, so a provisioned run would have created
`<store>/sources/…`. Nothing says the two sources were skipped.

CI's `check-template-copy-out` job (`.github/workflows/gate.yml`) works around
it by running the tool and the sources as two commands.

## Cause

`cmd/setup.rs::run` chose WHAT to do by the order of its early `return`s:
`--list`, `--licenses`, `--workspace`, `--system`, `--build-sources`,
`--check`, `--tool`, `--source`, then `<board>`. The `--tool` arm was
`return install_tools(..)`, so every selector after it on the line was dead.

## The class — every selector an earlier arm drops

Swept by reading each arm of `run` for what it reads and what it returns
before (`grep -nE 'return|args\.' packages/cli/nros-cli-core/src/cmd/setup.rs`
over `run`). Each of these exited 0 having ignored a selector:

| Given | Arm that ran | Dropped |
| --- | --- | --- |
| `--tool X --source Y` | install tools | the sources (reported) |
| `<board> --tool X` / `<board> --source Y` | install tools / sources | the board's whole package set |
| `--check --source Y` (± `--tool`) | doctor walk / tool pin check | the source |
| `--list` / `--licenses` / `--workspace` + anything | print / plan | everything else |
| `--system` + `--tool`/`--source`/`<board>` | system closure | the rest |
| `--build-sources` + `--tool`/`--source`/`<board>` | build-source union | the rest |
| `--rmw R` without `<board>` | whichever arm | the RMW's packages |
| `--prefix P` with only `--source` | sources | the prefix |
| `--role` without `--system` | whichever arm | the role filter |

`--sudo` alone was already refused (issue 1038).

Sibling verbs with more than one selector flag were read the same way
(`grep -cE '^\s*if (let Some\(.*\) = )?(!)?args\.[a-z_]+.* \{'` over
`packages/cli/nros-cli-core/src/cmd/*.rs`, ≥ 3 hits: `ws.rs`, `build.rs`,
`locate.rs`, `codegen.rs`, `toolchain.rs`, `image_facts.rs`,
`entity_census.rs`). None drops requested work: `locate` unions `<names>` with
`--all`, `ws sync --check` returns before `--dry-run` (both are read-only), and
`build --check` retargets the plan rather than returning.

## Resolution (2026-10-17)

`run` no longer decides by return order. `route()` (`cmd/setup.rs`) picks
the ONE arm from every selector on the line before the index is read, and
refuses — naming the flag — any selector that arm does not consume, so no
combination exits 0 over work it skipped. `--tool` + `--source` is one
request: `run_install` installs the tools, then provisions the sources; a
failed tool install stops before the sources with a non-zero exit.

Proof, same scratch-store measurement as above on the fixed CLI:

```
$ NROS_HOME=<scratch>/psetup-home-after \
    nros setup --tool arm-none-eabi-gcc --source freertos-kernel --source lwip
nros setup --tool arm-none-eabi-gcc: prebuilt 13.2-nros5 (dist linux-x86_64) → <scratch>/sdk/arm-none-eabi-gcc/13.2-nros5
nros setup --source freertos-kernel: source 10.6.2 — submodule third-party/freertos/kernel → <scratch>/sources/freertos-kernel/10.6.2+0adc196d [provisioned]
nros setup --source lwip: source 2.2.0 — submodule third-party/freertos/lwip → <scratch>/sources/lwip/2.2.0+6233eb61 [provisioned]
rc=0
$ nros setup linux --tool ninja
Error: nros setup: <board> 'linux' cannot be combined with `--tool`/`--source` (install the named packages) — …
```

Tests in `nros-cli-core` (the cli-tests lane), each made red by a mutation
and green again on restore: the Install arm runs both halves in order (red
under "return after the tools", the reported shape); 14 dropping
combinations are refused by name (red with the refusal disabled, with
Install consuming `<board>`, with the `--check` arm consuming `--source`);
14 consumed combinations still route.

The `check-template-copy-out` job in `.github/workflows/gate.yml` still runs
two commands; one would do now, and it is left as is.
