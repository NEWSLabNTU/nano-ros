---
id: 2499
title: "`just verify-verus` cannot run: the Verus binary, the crate's rust pin and the resolved `vstd` are three different versions"
status: open
type: bug
severity: medium
area: [verification, tooling]
related: [2487, rfc-0028]
found: 2026-11-13
---

**Found by** issue 2487's fix, which changed the spec of `integrity_is_valid`
in `packages/verification/nros-verification/src/safety.rs` and could not get a
Verus run to prove it. The lane fails before it reaches any nano-ros proof, so
every Verus proof in the tree is currently UNRUN, not only that one.

## What happens

Measured on 2026-11-13 in a fresh worktree:

1. `nros setup --tool verus` installs the `[tool.verus]` pin from
   `nros-sdk-index.toml`, `0.2026.06.28.1847ab3`. Its smoke check reports that
   the binary needs rust `1.96.0`.
2. After `rustup toolchain install 1.96.0-x86_64-unknown-linux-gnu --profile
   minimal`, `cargo verus verify` compiles `vstd` and rustc panics:

   ```
   thread 'rustc' panicked at rust_verify/src/verus_items.rs:750:17:
   unexpected verus diagnostic item verus::verus_builtin::atomic_spec
   error: could not compile `vstd` (lib)
   ```

## Why

Three things that must agree do not:

| Input | Where | Value |
| --- | --- | --- |
| Verus binary | `nros-sdk-index.toml` `[tool.verus]` | `0.2026.06.28.1847ab3`, wants rust 1.96.0 |
| rust toolchain | `packages/verification/nros-verification/rust-toolchain.toml` | `1.93.0`, its comment says it waits for a compatible Verus |
| `vstd` | `nros-verification/Cargo.toml` requires `0.0.0-2026-02-08-0120`; the build resolved `0.0.0-2026-07-27-0206` from crates.io | built by a different Verus than the binary |

`vstd` is built by the Verus binary itself, so its version must be the one
that binary was released with. The caret requirement lets cargo pick a newer
crates.io `vstd` whose diagnostic items the June binary does not know. That
is the panic.

## Fix direction

- Pin `vstd` (and `verus_builtin`/`verus_builtin_macros` if named) with `=`
  to the version released with the `[tool.verus]` binary, and move the
  rust-toolchain pin to the binary's `Toolchain:` line, all in one commit.
- A gate that reads the three and refuses a mismatch, so the next bump of one
  cannot strand the other two. The smoke check already reads `Toolchain:`.
- Then re-run `just verify-verus`, and confirm proof 5 of
  `nros-verification/src/safety.rs` (`integrity_is_valid` requires a checked
  CRC, from issue 2487) holds.

## Acceptance

`just verify-verus` exits 0 on a fresh host after `nros setup --tool verus`,
with no hand-installed toolchain.
