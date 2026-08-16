---
id: 629
title: "`riscv64-lld-wrapper.sh` adds no `-L`, so any C++ link on threadx-riscv64
  fails on `-lstdc++` / `-lnosys` the toolchain actually ships"
status: open
type: bug
area: build
related: [issue-0582, phase-366]
---

## Symptom

`just build-test-fixtures lane=tier2`, the `threadx-riscv64-c-cyclonedds` cell:

```
[54/55] Linking CXX executable c_talker
rust-lld: error: unable to find library -lstdc++
rust-lld: error: unable to find library -lnosys
```

## Mechanism

Both libraries are present in the provisioned toolchain:

```
~/.nros/sdk/riscv-none-elf-gcc/14.2-nros1/riscv-none-elf/lib/libnosys.a
~/.nros/sdk/riscv-none-elf-gcc/14.2-nros1/riscv-none-elf/lib/libstdc++exp.a
```

They are not found because nothing tells the linker where to look.
`cmake/toolchain/riscv64-threadx.cmake` replaces the link rule with
`riscv64-lld-wrapper.sh`, which filters a few gcc-only flags and then:

```sh
exec "$RUST_LLD" "${lld_args[@]}"
```

`gcc` would have passed its own `-L` set for the target; `rust-lld` invoked
directly has none. A C link never noticed, because nothing in that link line
names a library outside what the build already passes by path. A **C++** link
names `-lstdc++`, and the CycloneDDS cells link with the CXX driver
(`needs_cxx_linker` in the rmw resolver), so they are the first to need a search
path that was never there.

Note the multilib wrinkle any fix has to handle: the toolchain ships
`riscv-none-elf/lib/` plus per-ABI subdirectories (`rv32ea/ilp32e/…`), so the
correct `-L` depends on `-march`/`-mabi` and cannot be one hardcoded path. The
robust form is to ask the compiler — `riscv-none-elf-gcc -print-search-dirs`, or
`-print-file-name=libnosys.a` for the exact multilib — rather than compose the
path here, which is issue 0582's lesson in a different register.

## Not caused by phase-366, only unmasked by it

The cell built green as recently as this session's full `lane=tier2` sweep. It
was not REBUILT there — it was up to date, so nothing linked and nothing failed.
phase-366 edits `nros-platform-api/include/nros/platform.h`, which invalidates
every fixture that includes it, and the first rebuild of this cell surfaced a
link line that had been broken for as long as the C++ linker has been selected
for CycloneDDS.

That is the museum-binary hazard CLAUDE.md describes, in its exact shape: a
fixture passes freshness checks while carrying an artifact that the current tree
can no longer produce. Worth recording because the same masking can hide any
number of link-level regressions on the families that rebuild least often.

## Fix sketch

In `riscv64-lld-wrapper.sh`, derive the search paths from the compiler and pass
them through:

```sh
# the multilib-correct dir for the -march/-mabi this link is using
lld_args+=("-L$("$RISCV_GCC" "${arch_flags[@]}" -print-file-name=libnosys.a | xargs dirname)")
```

Alternatively, teach `nros_threadx_setup_rust_lld` to compute them once at
configure time and hand them to the wrapper by environment, matching how
`NROS_RUST_LLD` / `NROS_LLVM_AR` already reach it.

## Acceptance

`just threadx_riscv64 build-fixtures` builds the cyclonedds cells, and a C++
target links without a hand-passed `-L`.
