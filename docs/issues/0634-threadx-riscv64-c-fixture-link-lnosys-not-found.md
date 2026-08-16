---
id: 634
title: "`-lnosys` reaches the ThreadX RISC-V link line but its `-L` does not, so
  every C fixture on that lane fails to link"
status: open
type: bug
area: build, boards
related: [issue-0626, phase-358]
---

## Symptom

`just threadx_riscv64 build-all` fails in `build-fixture-extras`:

```
rust-lld: error: unable to find library -lnosys
ninja: build stopped: subcommand failed.
make: *** [.../threadx-riscv64-c-zenoh-all-…mk:16: fixture-0002] Error 1
```

Repeats for every C fixture on the lane. The Rust examples of the same lane
build and link fine (`ThreadX QEMU RISC-V examples built!`), which is what makes
this look platform-wide when it is not.

## Cause: one `if()` block, two halves that travel differently

`cmake/toolchain/riscv64-threadx.cmake` adds the newlib syscall stubs when it
finds them beside the SDK's libstdc++:

```cmake
if(_riscv_stdcxx)
    get_filename_component(_riscv_stdcxx_dir "${_riscv_stdcxx}" DIRECTORY)
    add_link_options("-L${_riscv_stdcxx_dir}")                       # (A)
    …
    if(EXISTS "${_riscv_stdcxx_dir}/libnosys.a")
        set(CMAKE_C_STANDARD_LIBRARIES "-lnosys" CACHE STRING "" FORCE)   # (B)
        set(CMAKE_CXX_STANDARD_LIBRARIES "-lnosys" CACHE STRING "" FORCE)
    endif()
endif()
```

**(B) survives and (A) does not.** `CMAKE_*_STANDARD_LIBRARIES` is a CACHE
variable, so it reaches every target in every subdirectory. `add_link_options()`
is a DIRECTORY-scope command, and a toolchain file is not the project's
directory scope — it is read early (and repeatedly, including inside
`try_compile`), so the option does not propagate to the targets the fixtures
create.

The actual failing link line shows exactly that asymmetry — it ends in
`-lnosys` while carrying only the picolibc search path:

```
… -L/usr/lib/picolibc/riscv64-unknown-elf/lib/rv64imafdc/lp64d \
  CMakeFiles/c_talker.dir/src/main.c.obj … -lc \
  /usr/lib/gcc/riscv64-unknown-elf/10.2.0/rv64imafdc/lp64d/libgcc.a \
  … libthreadx_kernel.a -lnosys
```

The library exists — the linker is simply never told where:

```
~/.nros/sdk/riscv-none-elf-gcc/14.2-nros1/riscv-none-elf/lib/rv64iafd_zicsr/lp64d/libnosys.a
```

and there is no `libnosys.a` (nor `libstdc++.a`) under
`/usr/lib/gcc/riscv64-unknown-elf`, so the `EXISTS` test passed on the SDK
directory — the one whose `-L` went missing.

## Why the Rust half of the lane is unaffected

The final link for a C fixture runs through `cmake/toolchain/riscv64-lld-wrapper.sh`
→ `rust-lld`, driven by CMake's link line, which is where (B) lands. The Rust
examples link through cargo/rustc, which never sees `CMAKE_C_STANDARD_LIBRARIES`
at all — so they neither gain the flag nor need the path. A lane that is half
green is the tell.

## Fix, in order of preference

1. **Emit both halves the same way.** Append the search path to the same cache
   variable that carries the library
   (`-L${_riscv_stdcxx_dir} -lnosys`), so the pair cannot separate. A library
   reference and its search path are one fact; storing them in two mechanisms
   with different lifetimes is the defect.
2. Or set `CMAKE_EXE_LINKER_FLAGS_INIT` (an `_INIT` variable IS the sanctioned
   way for a toolchain file to seed link flags) instead of `add_link_options()`.
3. Do NOT "fix" it by dropping `-lnosys`: it is there for a reason the file
   documents — the SDK's newlib-built libstdc++ references reent syscalls
   (`_sbrk`/`_read`/`_kill`) that nothing bare-metal provides, and it is
   appended last so the image's real malloc/IO resolve first.

Whichever is chosen, verify with the LINK LINE rather than a green build: the
`-L` and the `-lnosys` must appear together in
`build.ninja`'s command for a C fixture.

## Not

* **Not caused by issue 0626's ThreadX work.** That change adds no link flags —
  its diff is a typedef widening, `task.c`'s attr handling, a shim branch, a
  manifest knob and two cmake `zephyr_compile_definitions`. The Rust examples on
  the same lane build and link with it in place, and the failing step is the C
  fixture link. **A clean control was NOT run**, so this is reasoned from the
  diff rather than measured — worth doing before closing, since "my change
  didn't touch that" has been wrong here before.
* Not a missing SDK component: `libnosys.a` is present, in the expected multilib
  directory, for the ABI in use (`lp64d`).

## Found by

Phase-358 W3 → #0626, building the ThreadX lane to verify the transport-task
priority change end to end. The Rust half of that verification passed; this is
what the C half hit.
