# Contributing

This chapter covers the development workflow for nano-ros.

## Development Setup

This is the **contributor** setup: a checkout of the repository and the
CLI built from it. If you only want to *use* nano-ros, install a release
instead — [Installation](../getting-started/installation.md); every user
chapter of this book assumes the installed CLI and no checkout.

Inside a nano-ros checkout the tree's own build is the only correct
`nros`: a released `nros` run against a checkout is refused outright,
because it emits *its own* generated code and this tree's runtime is the
one that has to compile it
([RFC-0090](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0090-codegen-version-is-the-compatibility-token.md)).
**One checkout = one CLI version = one runtime ABI** — no global
install, no `~/.nros/bin` PATH skew across worktrees.

The host prerequisites are the ones in
[Installation § Host prerequisites](../getting-started/installation.md#host-prerequisites)
(`python3-dev` is needed here: `bootstrap.sh` builds the launch resolver,
which embeds CPython to parse ROS launch files).

### From a fresh checkout

```sh probe=20 track=checkout
# 1. Pull the source (`main`, or a release tag `nros-<version>`):
git clone https://github.com/NEWSLabNTU/nano-ros.git
cd nano-ros

# 2. Build the `nros` CLI from source (installs rustup if needed):
./scripts/bootstrap.sh

# 3. Wire the workspace env into this (and every future) shell:
source ./activate.sh          # or: direnv allow / source ./activate.fish

# 4. Provision the toolchains + RMW daemon for your target:
nros setup <board> --rmw <zenoh|xrce|cyclonedds>    # e.g. `linux`
```

`bootstrap.sh` builds the in-tree `nros` CLI **from source** at
`packages/cli/target/release/nros` and leaves it on PATH for that shell
(`just` is NOT a prerequisite; rustup is installed on demand). Already
have cargo? The equivalent by hand:

```sh
git submodule update --init packages/cli/third-party/play_launch
cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros
cargo build --release --manifest-path packages/cli/nros-launch-resolve/Cargo.toml
export PATH="$PWD/packages/cli/target/release:$PATH"
```

Both builds are needed for a workspace flow: `nros sync` and `nros
build` call `nros-launch-resolve` to resolve launch files, so a bare
`cargo build --bin nros` alone is not equivalent.

### Every subsequent shell

Every new shell needs the workspace env wired in. All three wire the
same env exports + PATH entries from `activate.sh`:

```bash
direnv allow                  # auto-activates on `cd nano-ros` (recommended)
source ./activate.sh          # bash / zsh, one-shot per shell
source ./activate.fish        # fish, one-shot per shell
```

`activate.sh` is the after-install step, not a prerequisite: it presumes
the `nros` binary already exists at `packages/cli/target/release/nros`.
It also sources `/opt/ros/humble/setup.bash` if present (used by the
rmw_zenoh interop tests, and it lets codegen resolve message packages
from the ament index). It exports no checkout root: the in-tree `nros`
knows the checkout it was built in. Without ROS 2 it says so:

```
activate.sh: /opt/ros/humble/setup.bash not found — ROS-dependent recipes
  will fail (interop tests, `ros2` CLI verification). The setup, codegen
  and first-node flows do not need it.
```

On a host without `just` it also prints one line about RTOS SDK path
defaults (`FREERTOS_DIR`, `NUTTX_DIR`, `THREADX_DIR`, …) not being
loaded — only the in-tree `just` recipes (test lanes, fixture builds)
read them (`cargo install just`).

### Provisioning through `just`

The `just` setup recipes are thin wrappers over the same `nros setup`
index — `just setup <module>` calls `nros setup <board>` under the hood,
so the toolchains a contributor gets are identical to a user's:

```bash
just setup freertos           # → nros setup mps2-an385-freertos
just setup nuttx              # → nros setup qemu-armv7a-nuttx
just setup threadx_linux      # → nros setup threadx-linux
just doctor tier=all          # diagnose missing tools (read-only)
```

A checkout's version is its tag: the CLI built from a `nros-<version>`
tree prints the same number (`scripts/check-version-lockstep.sh` and
`bootstrap.sh shell-doctor` enforce binary↔tree agreement), and the
tree's committed `nros-sdk-index.toml` pins the toolchains. Upgrading =
checking out a newer tag, re-running `./scripts/bootstrap.sh`, then
`nros setup`.

### Docker environment

For a containerized environment with QEMU 7.2+ (in-tree checkout):

```bash
just docker build
just docker shell      # Interactive shell with all tools
just docker test-qemu  # Run QEMU tests in container
```

### Setup tiers

Choose the setup tier. `base` is enough for most code review and
native development; `all` prepares the full `test-all` matrix:

```bash
just setup               # print setup choices
just setup base          # workspace tools + zenohd
just setup all           # all supported SDKs/services
source ./activate.sh
```

> **The test and fixture stages need the vendored-source UNION, not one board.**
> `nros setup <board> --rmw <x>` provisions one board+rmw's sources — right for
> *building an app* for that board. But `just test` links every RMW's `-sys`
> crate and `just build-test-fixtures` resolves component graphs that path-dep
> platform sources (e.g. `nuttx/libc`) even for a native component, so the
> contributor build needs the whole set. Both recipes preflight it and, if a
> source is missing, name the exact remedy — `nros setup --source <name>`, or
> `nros setup --build-sources` to provision the union at once (issue 0390).

Build the entire workspace including examples:

```bash
just build
```

Use groups to inspect the command surface:

```bash
just --group main --list
just --group full-matrix --list
just --group full-matrix --list zephyr
```

## Quality Checks

**Always run `just ci` after completing a task that needs the full
maintainer gate.** This runs the normal checks plus the full test matrix
entry points:

```bash
just ci
```

For a narrower local iteration, run `just check` and `just test`.

For platform-specific CI, use `just <platform> ci` (e.g., `just freertos ci`).

## Testing

nano-ros has several test tiers, each with its own just recipe:

| Recipe | What it tests | External deps |
|--------|--------------|---------------|
| `just test-unit` | Unit tests (no external deps) | None |
| `just test-integration` | Rust integration tests, no heavy QEMU/Zephyr (builds zenohd automatically) | None |
| `just test` | `test-unit` + `test-integration` (default dev tier) | None |
| `just test-doc` | rustdoc doctests for the `nros` umbrella crate | None |
| `just test-miri` | Undefined behavior detection (standalone) | None |
| `just qemu test` | QEMU bare-metal examples | `qemu-system-arm` |
| `just freertos test` | FreeRTOS QEMU E2E | `qemu-system-arm` + `arm-none-eabi-gcc` |
| `just nuttx test` | NuttX QEMU E2E | nightly + `qemu-system-arm` |
| `just threadx_linux test` | ThreadX Linux user-space E2E | `gcc` |
| `just threadx_riscv64 test` | ThreadX RISC-V QEMU E2E | `qemu-system-riscv64` |
| `just zephyr test` | Zephyr E2E (native_sim + NSOS) | west |
| `just test-all` | Everything: `test` + heavy QEMU/Zephyr/ROS-interop + `test-doc` + `test-miri` + C codegen | All of the above |

All test recipes accept a `verbose` argument for live output.

### Test organization

- Reusable Rust tests go in `packages/testing/nros-tests/tests/`
- Shell-based test scripts go in `tests/` with corresponding justfile entries
- Temporary tests can be run directly in the shell, then converted to proper tests once validated

### Build isolation

Nextest runs each test file as a separate process in parallel. When multiple tests build the same example with different features, use `--target-dir` to isolate output directories (e.g., `target-safety/`, `target-zero-copy/`). Add new target dirs to the example's per-directory `.gitignore`.

## Code Style

### Rust Edition 2024

nano-ros uses Rust edition 2024, which requires:

- `unsafe extern "C" { ... }` -- extern blocks require the `unsafe` keyword
- `#[unsafe(no_mangle)]` -- `no_mangle` requires the `unsafe` attribute
- Unsafe operations inside `unsafe fn` need explicit `unsafe { ... }` blocks

The `nros-c` crate keeps `#![allow(unsafe_op_in_unsafe_fn)]` due to 420+ FFI operations.

### Unsafe conventions

- Minimize unsafe usage; prefer safe abstractions
- Document safety invariants with `// SAFETY:` comments
- Use wrapper types (like `SubscriberBufferRef`) to encapsulate unsafe access patterns

### `no_std` patterns

All core crates support `#![no_std]` with optional `std`/`alloc` features:

- `#[cfg(feature = "alloc")]` gates `Vec`/`Box` usage
- `#[cfg(feature = "std")]` gates std-only features
- `heapless::Vec` is used in `no_std` contexts

### Unused variables

- Rename to `_name` with a comment explaining why
- Use `#[allow(dead_code)]` for test struct fields

## Message Types

Message types are always generated, never hand-written. Use the codegen tool:

```bash
nros generate-rust
```

Bundled interface definitions live in `packages/cli/interfaces/`. Example `generated/` directories are gitignored and recreated by `just generate-bindings`. Only the core pre-generated set checks its `generated/` into git: ONE driver package at `packages/interfaces/`, one output tree at `packages/interfaces/generated/humble/`, six crates, regenerated with `just generate-interfaces` on a ROS 2 ament host.

## System Packages

**Never install system packages or run sudo directly.** If a system dependency is needed, document what the user should install.

## Temporary Files

Create temporary files in the project's `tmp/` directory (git-ignored), not in `/tmp`.

## PR Workflow

1. Create a branch from `main`
2. Make changes
3. Run `just ci` -- all checks must pass
4. Submit for review

## Verification

For changes to core crates, run the formal verification suite:

```bash
just verify          # Kani + Verus
just verify-kani     # Kani bounded model checking (~3 min)
just verify-verus    # Verus deductive proofs (~1 sec)
```
