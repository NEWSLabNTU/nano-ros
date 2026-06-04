# CLI lives in the monorepo (Phase 218)

The `nros` CLI lives in-tree at `packages/cli/` as a **sub-workspace**
— own `Cargo.toml`, own `Cargo.lock`, own member crates (`nros-cli`,
`nros-cli-core`, `nros-build`, `cargo-nano-ros`, `rosidl-*`,
`colcon-cargo-ros2`, `nros-msg-to-idl`). It is built per checkout via
`just setup-cli` → `packages/cli/target/release/nros`, then put on
`PATH` by the activate file. See also:

- [Phase 218 roadmap doc](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/roadmap/phase-218-merge-cli-into-monorepo.md)
- [Phase 218 design spec](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/superpowers/specs/2026-06-04-cli-monorepo-merge-design.md)

## Why sub-workspace, not root member

The runtime workspace's `no_std` feature-unification view (Phase
214.F.3) is the project's most fragile invariant — a single workspace
member adding a `std`-activating dep without target-gating turns the
embedded build red on a path the test matrix doesn't always exercise.
The CLI's deps (`clap`, `askama`, `syn`, `ureq`, …) are aggressively
`std`-only; making them workspace siblings of the runtime crates would
either (a) require target-gating every single one or (b) effectively
disable the F.3 guard.

The sub-workspace shape keeps the two surfaces categorically separate.
The pattern mirrors `packages/testing/nros-{tests,bench,smoke}/`,
which carved out the same way for the same reason.

## Why per-checkout, not `~/.nros/bin`

Confirmed during the Phase 218 UX walkthrough: contributors with
multiple nano-ros worktrees (phase branches, ASI integration trees,
downstream forks) need each tree to point at its own CLI. A global
install would silently version-skew across trees the moment the user
`cd`s — and because the CLI's codegen format is structurally bound to
the `nros-core` / `nros-c` / `nros-cpp` crates in the same checkout, a
global CLI is a known footgun.

The per-checkout shape makes the activate file the single switch — no
`which nros` surprise across cwd boundaries. **One checkout = one CLI
version = one runtime ABI**, by construction; no version-pin matrix to
police.

## Resolution chain

`scripts/build/cargo.sh::nros_cli_bin` resolves the active binary:

1. `$NROS_CLI` (explicit override)
2. `nros` on `$PATH` (activate file puts the repo-local one here)
3. `packages/cli/target/release/nros` (per-checkout)
4. `${NROS_HOME:-~/.nros}/bin/nros` (**transitional**, removed once
   every active branch lands on 218)
5. error

The Phase 218.E ABI guard checks the in-tree CLI build matches the
runtime crates' codegen ABI hash; opt out via `NROS_SKIP_VERSION_CHECK=1`
when intentionally bisecting a CLI mismatch.

## See also

- [Build System & Caching](./build-system.md)
- [SDK Tiers](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/development/sdk-tiers.md)
- [Environment Variables](../reference/environment-variables.md)
