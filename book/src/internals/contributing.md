# Contributing

This chapter covers the development workflow for nano-ros.

## Development Setup

Install all toolchains, cargo tools, and download third-party SDKs (FreeRTOS, NuttX):

```bash
just setup
```

Build the entire workspace including examples:

```bash
just build
```

## Quality Checks

**Always run `just ci` after completing a task.** This runs formatting checks, Clippy lints, unit tests, Miri, and QEMU examples in one command:

```bash
just ci
```

It is equivalent to running `just check` + `just test` sequentially.

For platform-specific CI, use `just <platform> ci` (e.g., `just freertos ci`).

## Testing

nano-ros has several test tiers, each with its own just recipe:

| Recipe | What it tests | External deps |
|--------|--------------|---------------|
| `just test-unit` | Unit tests (no external deps) | None |
| `just test-miri` | Undefined behavior detection | None |
| `just test-qemu` | QEMU bare-metal examples | `qemu-system-arm` |
| `just test-integration` | Rust integration tests (builds zenohd automatically) | None |
| `just test` | unit + miri + qemu + integration | `qemu-system-arm` |
| `just test-zephyr` | Zephyr E2E | west + TAP |
| `just test-freertos` | FreeRTOS QEMU E2E | `qemu-system-arm` + `arm-none-eabi-gcc` |
| `just test-nuttx` | NuttX QEMU E2E | nightly + `qemu-system-arm` |
| `just test-ros2` | ROS 2 interop | ROS 2 + rmw_zenoh |
| `just test-c` | C API tests | cmake |
| `just test-all` | Everything | All of the above |

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
cargo nano-ros generate-rust
```

Bundled interface definitions live in `packages/codegen/interfaces/`. Example `generated/` directories are gitignored and recreated by `just generate-bindings`. Only `packages/interfaces/rcl-interfaces/generated/` is checked into git.

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
