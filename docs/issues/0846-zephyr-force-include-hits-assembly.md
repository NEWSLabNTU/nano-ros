---
id: 846
title: "zephyr_compile_options(-include <C header>) reaches ASSEMBLY targets and
  breaks every Zephyr image with CONFIG_NROS_CPP_API"
status: resolved
type: bug
area: zephyr
related: [phase-394]
---

## Symptom

Any Zephyr image built with `CONFIG_NROS_CPP_API` fails to compile Zephyr's own
architecture assembly:

```
/…/aarch64-zephyr-elf/sys-include/machine/_default_types.h: Assembler messages:
…:41: Error: unknown mnemonic `typedef' -- `typedef …'
FAILED: zephyr/arch/arch/arm64/core/CMakeFiles/arch__arm64__core.dir/early_mem_funcs.S.obj
```

Measured on `fvp_baser_aemv8r_smp` (aarch64) via the Autoware Safety Island
consumer, where it broke the image build outright — the lane went from a
working closed-loop demo to no build at all.

## Cause

`44bdc2157` added a force-included minimal-libc shim:

```cmake
zephyr_compile_options(-include ${NROS_REPO_DIR}/zephyr/libc-compat/nros_libc_compat.h)
```

`zephyr_compile_options()` applies to EVERY target in the image, and Zephyr
compiles `.S` sources through the same driver. `-include` on an assembly TU
feeds the assembler a C header, so the first `typedef` it meets is read as an
instruction. Nothing about the shim is wrong; its REACH is.

The shim's own `#ifdef _IONBF` guard cannot help here — that is a preprocessor
guard, and the file is still fed to the assembler either way.

## Fix

Gate the flag on the compile language. Note the flag and its argument must be
separate list elements: inside a generator expression a space would make them a
single argument, which gcc rejects.

```cmake
zephyr_compile_options(
    "$<$<COMPILE_LANGUAGE:C,CXX>:-include>"
    "$<$<COMPILE_LANGUAGE:C,CXX>:${NROS_REPO_DIR}/zephyr/libc-compat/nros_libc_compat.h>")
```

Verified: `fvp_baser_aemv8r_smp` builds again, and the consumer's full Autoware
driving demo completes on it (route ARRIVED, peak 2.46 m/s).

## Why it was not caught

The commit that introduced it verified a `native_sim/native/64` image. That
target's arch layer contributes no assembly through this path, so the flag had
nothing to break there. A single cross target in the check would have caught
it — this is the class where "verified on one platform" and "verified" differ,
and the platform that differs is the one with real startup assembly.
