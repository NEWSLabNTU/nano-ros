---
id: 743
title: "the NuttX kernel path is one filename for two architectures, and nothing observes the clobber"
status: resolved
type: bug
area: build
related: [issue-0405, issue-0196, phase-329]
---

## Symptom

On 2026-08-21 a full sweep reported:

```
nros-tests::nuttx_qemu test_nuttx_kernel_boots
qemu-system-arm: Couldn't load elf '/home/aeon/repos/nano-ros/third-party/nuttx/nuttx/nuttx':
The image is from incompatible architecture
```

The file was a **RISC-V** ELF, five days old:

```
$ file third-party/nuttx/nuttx/nuttx
ELF 32-bit LSB executable, UCB RISC-V, RVC, soft-float ABI, ... Aug 16 14:07
```

The test is an ARM test. It was handed a RISC-V kernel and reported the
mismatch as a nano-ros test failure.

## Root cause

`nros_tests::fixtures::nuttx::nuttx_kernel_path()` resolves the kernel by
existence and nothing else:

```rust
pub fn nuttx_kernel_path() -> Option<PathBuf> {
    std::env::var("NUTTX_DIR").ok()
        .map(|dir| Path::new(&dir).join("nuttx"))
        .filter(|p| p.exists())          // exists — never "is it ARM?"
}
```

`$NUTTX_DIR/nuttx` is a single filename that BOTH the arm (`qemu-armv7a`) and
the riscv (`rv-virt`) NuttX configurations write. Whichever configuration was
built last owns it. A riscv NuttX build therefore costs an arm↔rv-virt kernel
reconfigure, and until someone reconfigures back, every arm consumer of that
path is holding a RISC-V binary that satisfies `.exists()`.

This is the same shape as issue 0196 one layer over: a probe whose predicate is
narrower than the property it is standing in for.

The MECHANISM was already known. Resolved issue 0405 says it plainly — "the
shared kernel tree holds one board config at a time and each reconfigures it" —
and split `build-fixtures-{arm,riscv}` so a lane naming no riscv coordinate does
not pay for a round trip. What 0405 fixed was the COST of the reconfigure to the
build lanes. What it left is the consumer side: nothing downstream can tell
which of the two configurations it is holding, because the resolver's only
question is `.exists()`.

## Why it is now unobserved

`test_nuttx_kernel_boots` was the SOLE consumer of that path as a boot target —
`rtos_e2e` touches `nuttx_kernel_path()` only as a skip probe, and boots
per-example images from `examples/qemu-arm-nuttx/<lang>/<role>` instead.

That test has been removed (test-cleanup pass, 2026-08-21) because as coverage
it was fully subsumed: all nine `rtos_e2e` `Platform::Nuttx` cells boot a real
nros kernel and assert DELIVERY, which cannot happen unless the kernel booted.
What it uniquely did was *notice this clobber* — and it reported it as a product
failure, which is what made it worth removing rather than keeping.

So the defect is unchanged and now silent: the wrong-arch artifact is still
produced, nothing looks at it.

## Fix direction

Give the two configurations **distinct paths** — resolve the kernel per arch
(e.g. `$NUTTX_DIR/nuttx-<arch>`, or an arch-qualified fixture artifact dir the
way `row_artifact_root()` attributes every other fixture) so an arm consumer
cannot be handed a riscv image at all.

**Do NOT fix this by restoring a boot probe.** A probe only re-reports a
build-tree defect as a test red; it does not stop the clobber, and it puts the
report in the place least able to explain it.

## Resolution (2026-08-21)

The resolver now ASKS THE FILE instead of trusting the name. `nuttx.rs` gained a
`NuttxArch` enum and

```rust
pub fn nuttx_kernel_path_for(arch: NuttxArch) -> Result<PathBuf, String>
```

which reads `e_machine` out of the ELF header (honouring `EI_DATA`, so a
big-endian header is not misread as `0x2800`) and refuses a kernel that is not
the architecture the caller asked for. Both consumers — `rtos_e2e`'s
`Platform::Nuttx` precondition and `logging_smoke`'s `_nuttx_qemu_arm` lane —
now name `NuttxArch::Arm`. The old bare `nuttx_kernel_path()` is gone, so there
is no spelling left that can answer the `.exists()` question alone.

The failure is now an unmet PRECONDITION reported in 0.1 s, naming the fix,
rather than a 10 s QEMU boot that dies with `The image is from incompatible
architecture`. Verified against the actual clobbered tree (which still held the
riscv image at the time of the fix):

```
[SKIPPED] the NuttX kernel at …/third-party/nuttx/nuttx/nuttx is a RiscV image,
but this lane needs Arm (qemu-armv7a). The arm and riscv configurations share
that ONE filename and each `make` reconfigures the tree (issue 0743), so the
last build wins. Reconfigure and rebuild: just nuttx build-fixtures-arm
```

Only the riscv half of that is reproducible against a real tree — proving the
arm half needs a reconfigure — so both directions are pinned by unit tests on
synthetic ELF headers (`elf_machine_distinguishes_arm_from_riscv`,
`elf_machine_reads_big_endian_headers`, `a_non_elf_is_not_a_kernel`).

**Not done, deliberately:** per-arch kernel FILENAMES (`nuttx-arm` /
`nuttx-riscv`). That would let both configurations coexist and skip the
reconfigure round trip, but it is a build-layout change, and resolved 0405
already gated the round-trip cost at the lane level. The defect this issue is
about — a consumer unable to tell which image it holds — is closed by the
resolver: with the arch check in place, staging could not change any verdict,
only the number of rebuilds.

### Fallout found while fixing it

`.config/nextest.toml` had two overrides filtering on `binary(nuttx_qemu)` and
`binary(threadx_linux)`. Those targets were deleted in the same pass, and a
`binary()` naming a missing target is **not** inert the way a stale `test()`
name is — nextest fails to PARSE the config, so EVERY nextest run in the repo
errored (`failed to parse profile.default.overrides at index 9`) until both were
narrowed to their `rtos_e2e` disjunct. Worth remembering: `just check` does not
run nextest, so it stayed green throughout.

That gap is now closed by `check-nextest-binary-filters` (in `just check`),
which parses the config with a TOML parser — a grep cannot distinguish a live
filter from a comment ABOUT a removed one, and the first pass at this audit
duly mis-reported the commented-out `binary(dds_api)` / `binary(dds_ros2_interop)`
notes as live dead references. Run against the pre-fix config it reproduces
nextest's own verdict exactly, down to the indices: `overrides[9]`
`binary(nuttx_qemu)` and `overrides[13]` `binary(threadx_linux)`.

The gate covers `binary()` only. `test()` names are rstest-generated case names
(`Platform__Nuttx`, `zenoh_rust_pubsub_e2e`) that appear literally nowhere in
the test sources, so checking them needs a compiled `cargo nextest list` — too
heavy for `just check`. That half degrades quietly (an inert override just stops
applying its timeout/retries) rather than taking the repo down, so the gate
covers the fatal half exactly instead of the quiet half approximately.
