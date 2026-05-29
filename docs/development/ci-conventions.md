# CI conventions for nano-ros

A new GitHub Actions workflow should work on its **first** push. The Phase 196
bring-up of `zephyr-dual-line.yml` cost seven stacked CI rounds because each
workflow re-discovered the same fresh-runner gaps (no submodules, no ROS, no
Python 3.12, wrong runner OS). This page is the checklist + copy-paste snippets
that close that gap class. Pair it with the live workflows in
`.github/workflows/` — they are the worked examples.

## The mental model: the runner is a fresh clone, nothing else

A GitHub runner has the repo at the recorded commit and the runner image's
default tools. It has **no** submodules, **no** ROS, **no** SDKs, and an OS-pinned
Python. Every assumption your laptop satisfies silently is a CI failure. Walk the
job as if you just cloned into an empty container.

## Conventions

### 1. Init only the submodules you need — never recursive-all

`actions/checkout@v4` initializes **no** submodules. Init the specific paths the
job touches; never `submodules: recursive` on checkout (the platform/RTOS
submodules are large and fork-pinned — a full recursive init is slow and pulls
trees the job will never read).

```yaml
- uses: actions/checkout@v4
- name: Init needed submodules
  run: git submodule update --init --recursive packages/codegen
  # add others a job actually builds, e.g.:
  #   third-party/dds/cyclonedds packages/zpico/zpico-sys/zenoh-pico
```

**The rule — provision via `nros`, not hand `git submodule update`.** A *user*
does not `git submodule update` their libc / zenoh-pico / cyclonedds fork; they
run `nros setup`, which provisions sources from `nros-sdk-index.toml`. CI must
simulate that. The **only** hand-init allowed is the bootstrap that `nros` itself
can't provision: `packages/codegen` (the CLI's own source — chicken/egg). Then:

```yaml
- name: Init codegen submodule (bootstrap for the nros CLI)
  run: git submodule update --init --recursive packages/codegen
- name: Build the nros CLI
  run: cargo build --manifest-path packages/codegen/packages/Cargo.toml -p nros-cli --bin nros
# board's whole toolchain + source set:
- run: packages/codegen/packages/target/debug/nros setup <board> --rmw <rmw>
# or a specific source (submodule) by name:
- run: packages/codegen/packages/target/debug/nros setup --source <name> [--source <name>…]
```

If a build needs a source the index doesn't provision, **that is an index/`nros`
bug to fix** (add the `[source.*]` entry / teach `nros`), never a `git submodule`
line in the workflow. Examples: `dep-chain.yml` (per-board), `ci.yml` (px4-rs via
`--source`), `zephyr-dual-line.yml` (zenoh-pico + cyclonedds-src via `--source`).

### 2. ROS 2 for the interface codegen

`nros generate-rust` / `nros codegen` resolve a message package's `msg/*.msg`
via `AMENT_PREFIX_PATH` from a sourced ROS 2. Provide it and `source` before any
build that codegens interfaces:

```yaml
- name: Install ROS 2 Humble
  uses: ros-tooling/setup-ros@v0.7
  with:
    required-ros-distributions: humble
# ... then in the build step:
- run: source /opt/ros/humble/setup.bash && <build command>
```

### 3. Runner OS follows the ROS distro

`ros-tooling/setup-ros` keys off the runner OS. Humble's baseline is **jammy**, so
any job that sources ROS 2 Humble must `runs-on: ubuntu-22.04` (not the floating
`ubuntu-latest`, which has moved to noble). Pure-lint jobs that need no ROS can
stay on `ubuntu-latest`.

### 4. Python 3.12 via `uv` for the Zephyr line

Zephyr 4.4 needs Python ≥3.12; `scripts/zephyr/provision-py312-venv.sh` requires
`uv` with no fallback. Add it before the Zephyr setup:

```yaml
- uses: astral-sh/setup-uv@v5
```

### 5. Build the `nros` CLI from source — the published bin is stale

The crates.io `nros` (0.2.0) predates `nros setup`. Any workflow driving the CLI
must build the current one from the `packages/codegen` submodule and invoke that
binary, not a `cargo install`ed one:

```yaml
- run: cargo build --manifest-path packages/codegen/packages/Cargo.toml -p nros-cli --bin nros
# then reference packages/codegen/packages/target/debug/nros
```

### 6. Canonical codegen invocation: `nros codegen --args-file`

The low-level codegen entrypoint is the `codegen` subcommand
(`nros codegen --args-file …`); the old top-level `nros --args-file …` was
removed in Phase 195. Build glue (`*.cmake`, `*.just`, `*.sh`) that drops the
subcommand silently breaks interface generation. The
`codegen-convention.yml` lint (`scripts/ci/codegen-invocation-check.sh`) enforces
this — don't reintroduce the bare form.

### 7. Concurrency: cancel-in-progress per ref

Push-storms (a maintainer landing several commits) otherwise queue redundant
runs. Cancel the in-flight run for the same ref:

```yaml
concurrency:
  group: <workflow-name>-${{ github.ref }}
  cancel-in-progress: true
```

Note the side effect: rapid pushes show **cancelled** runs for the superseded
commits — that is the dedup working, not a failure. Judge a commit by the run on
*its own* SHA.

### 8. Path-filter triggers to what the workflow proves

Scope `on.push`/`on.pull_request` `paths:` to the inputs the job actually
validates, plus the workflow file itself. Keeps the matrix off unrelated commits.
Always include `workflow_dispatch:` so a workflow can be re-run by hand.

## Cost discipline

- **Validate the dep chain, don't rebuild the world.** Full per-platform builds
  are expensive; `dep-chain.yml` proves every `(board, rmw)` cell *resolves*
  (`nros setup` + codegen + `cargo tree`, no compile) in one cheap job. Reserve
  full builds for the sparse lanes (`zephyr-dual-line`, `just build-all`).
- **Cache the heavy SDKs.** A job that re-installs the ~1 GB Zephyr SDK and
  west-updates from scratch every run should cache `scripts/zephyr/sdk` + the
  workspace (Phase 196.3 follow-up).

## Preconditions must fail loud

Mirroring the repo-wide test rule (`CLAUDE.md`): a CI step whose precondition is
unmet must **exit non-zero**, never warn-and-pass. `scripts/ci/*` check
`AMENT_PREFIX_PATH`, the `nros` binary, etc. up front and `exit 1` with a fix
hint. A green check must mean the thing was actually validated.

## Split CI into a core lane + per-platform lanes

CI is split so each workflow provisions only what it validates — keeping
per-workflow minutes low and failures isolated to one platform:

- **Core-libraries lane** (`ci.yml`, job `core-libs`) — the portable `no_std`
  core crates cross-checked on bare embedded targets. No SDKs, no submodules; the
  only setup is a `rustup target add`. Split by target (one job per target,
  parallel), each a single `cargo check` over the crates compatible with it
  (e.g. `nros-rmw-cffi` needs atomic CAS, so it's checked on `thumbv7m` but not
  `riscv32imc`).
- **Cross-platform resolution lane** (`dep-chain.yml`) — proves every
  `(board, rmw)` dep chain *resolves* via one cheap job (`nros setup` per board
  pulls only that board's tools; ROS installed once). No compiles.
- **Per-platform build lanes** — one workflow per platform, each pulling only
  that platform's SDK + submodules (`zephyr-dual-line.yml` is the template). The
  heavy lanes; add a new one per platform rather than fattening a shared job.

## The worked examples

| Workflow | What it shows |
|----------|---------------|
| `ci.yml` (core-libs) | the core no_std lane: per-target matrix, rustup-only setup, no submodules |
| `dep-chain.yml` | submodule-minimal init, ROS source, build-CLI-from-source, the dep-chain matrix |
| `zephyr-dual-line.yml` | the full fresh-runner stack: submodules + uv + ROS + jammy + skip-flags + SDK cache |
| `codegen-convention.yml` | a pure static lint (no toolchain) on `ubuntu-latest` |
| `sdk-index-gate.yml` | offline structural validation of `nros-sdk-index.toml` |
