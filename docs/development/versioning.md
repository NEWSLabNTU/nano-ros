# nano-ros versioning

**Model: JetPack-style bundle.** One version number covers the whole
project — runtime crates, the `nros` CLI, the C/C++ surfaces, the
example trees, the board-glue cmake. A release bundles the artifacts
behind that number; consumers either:

- clone the repo at a `nros-v<X.Y.Z>` tag (runtime + examples + cmake),
- AND/OR fetch the matching CLI binary from the GitHub release page
  (the Phase 218.G `nros-<triple>.tar.gz` assets).

The model is named after NVIDIA's JetPack: a single SDK version
(JetPack 5.1.2) covers CUDA + cuDNN + TensorRT + the L4T kernel + the
rest — distributed as one bundle, not as N independently-versioned
crates on PyPI.

## Why not crates.io publish?

The runtime crates carry deep C/C++ deps via `*-sys` shims —
zenoh-pico, Cyclone DDS, micro-XRCE-DDS, mbedtls, FreeRTOS/Zephyr/
NuttX/ThreadX RTOS surfaces. crates.io can't carry those cleanly:
build scripts assume a configured workspace with SDK paths exported
(see `.envrc` / `activate.sh`), and the SDK source itself is gitignored
+ index-driven (`nros-sdk-index.toml`).

A user who tries `cargo add nros-core` from a third-party project gets
a crate that won't link in any meaningful environment. Better to ship
the whole thing as a bundle, where the SDK provisioning,
the codegen toolchain, the runtime, and the examples all move together.

**No nano-ros crate is published to crates.io.** Every workspace
member should carry `publish = false` (defensive; prevents accidental
publish on a stray `cargo publish`).

Consumers pin nano-ros runtime crates through their project's
`[patch.crates-io]` table, redirected to a local path (or to a git
submodule of the tagged release). The Phase 212.M-F.21 `nros ws sync`
verb writes those entries automatically.

## Single workspace.package.version

Two `Cargo.toml` files carry a `[workspace.package].version` field:

| File | Role |
|---|---|
| `Cargo.toml` | runtime workspace (nros-core, nros-c, nros-cpp, boards, RMW shims, …) |
| `packages/cli/Cargo.toml` | CLI sub-workspace (nros, nros-cli-core, nros-build, rosidl-*, codegen) |

These two MUST stay in lockstep. `scripts/check-version-lockstep.sh`
(wired into `.github/workflows/lint.yml`) errors if they diverge.
`just release-bump <X.Y.Z>` updates both atomically.

Inside each workspace, every member crate inherits the version via
`version.workspace = true`. New crates added to either workspace
should follow this pattern.

## Bumping the bundle

Three rules:

1. **Patch bump (`0.4.0` → `0.4.1`)** — any non-ABI-breaking change to
   ANY subsystem (CLI bug fix, runtime perf fix, board-glue cleanup,
   docs-only changes that ship with release notes).
2. **Minor bump (`0.4.x` → `0.5.0`)** — any ABI-affecting change to
   `nros-core`, `nros-c`, `nros-cpp`, or any of the RMW vtable
   surfaces. The Phase 218.E ABI guard will reject pairings that
   straddle a minor boundary, forcing consumers to update.
3. **Major bump (`0.x.y` → `1.0.0`)** — declared stability commitment;
   reserved for the post-rclrs-0.7-parity moment.

Patches are cheap (no crates.io republish). Resist the temptation to
batch unrelated changes into a single bump just to "save a version
number" — the bundle model treats version numbers as low-cost labels.

### Bump workflow

```sh
just release-bump 0.4.1                # sed both files + lockstep check
git diff Cargo.toml packages/cli/Cargo.toml
git commit -am 'release: nros-v0.4.1'
git tag nros-v0.4.1
git push origin main nros-v0.4.1       # triggers release.yml
```

The CI lane (`release.yml`) builds the CLI for four target
triples (linux+macos × x86_64+aarch64) and attaches the tarballs to
the GitHub release that the tag created.

## ABI guard (Phase 218.E)

`nros codegen` / `nros generate-rust` reads the consumer's
`Cargo.lock`, finds the resolved `nros-core` version, and compares it
to the CLI binary's compile-time `env!("CARGO_PKG_VERSION")`. Strict
equality. Mismatch → exit non-zero with both versions printed.

`NROS_SKIP_VERSION_CHECK=1` opts out (warned to stderr for CI
visibility).

The strict comparison is the bundle model's enforcement mechanism:
if a consumer has `nros-core 0.4.0` in their lockfile and runs the
`nros 0.5.0` binary, the codegen / runtime ABI assumption is broken
and the build SHOULD fail loud. The maintainer's job (via
`just release-bump`) is to keep the CLI and the runtime moving as one
unit so the guard stays quiet on the tracked tag.

When the guard fires within the nano-ros tree itself (during local
development on a branch where the version hasn't been bumped yet), the
`NROS_SKIP_VERSION_CHECK=1` env in your shell session is the documented
relief valve.

## Baseline

Bundle version `0.4.0` is the Phase 218 monorepo-merge baseline. It
was chosen so that:

- It signals a discontinuity from `nros-v0.3.7` (the last standalone
  nros-cli tag — see the archived
  `github.com/NEWSLabNTU/nros-cli` history).
- It doesn't claim post-1.0 stability the project hasn't yet
  established.
- `0.x.y` SemVer convention (everything pre-1.0 is "API may change")
  matches the current development pace.

## Related

- Phase 218 doc: `docs/roadmap/phase-218-merge-cli-into-monorepo.md`
- Phase 218 design spec:
  `docs/superpowers/specs/2026-06-04-cli-monorepo-merge-design.md`
- ABI guard impl: `packages/cli/nros-cli-core/src/abi_guard.rs`
- Lockstep check: `scripts/check-version-lockstep.sh`
- Bump recipe: `just release-bump` in the root `justfile`
- Release workflow: `.github/workflows/release.yml`
