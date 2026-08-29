---
id: 882
title: "`cmake --build --target <exe>` DELETES the NuttX kernel it is documented
  to produce — two producers write one path and only one can succeed"
status: resolved
type: bug
area: cmake, boards
related: [issue-0475, issue-0870, phase-385]
---

## The claim, and the measurement that contradicts it

`cmake/board/nano-ros-board-nuttx-qemu-arm.cmake:394` documents the arrangement:

>     EXCLUDE_FROM_ALL — keep it out of the default build
>     add_dependencies(carrier <name>_build) — `cmake --build . --target <name>`
>       still produces the kernel ELF via the cargo path

It does not. Run against a good tree:

    $ sha256sum build-zenoh/cpp_action_client        # 7196a77b40bc…, 826 620 bytes
    $ cmake --build build-zenoh --target cpp_action_client
    …
    collect2: error: ld returned 1 exit status
    ninja: build stopped: subcommand failed.
    rc=1
    $ ls build-zenoh/cpp_action_client
    ls: cannot access …: No such file or directory

The command fails AND takes the working kernel with it, because ninja deletes a
failed rule's output.

## Why

Two producers write `build-zenoh/<exe>`:

* `build <exe>: CXX_EXECUTABLE_LINKER…` — `main.cpp` plus the message archives
  and nothing else. It CANNOT link: `undefined reference to _lseek / _read /
  _exit / _kill / _getpid`, the NuttX syscalls that only the cargo path
  supplies. The comment above it already knows this, which is why the target is
  `EXCLUDE_FROM_ALL`.
* `CMakeFiles/<exe>_build.util` — `cmake -E copy nros-nuttx-ffi-out/nros-nuttx-ffi
  <exe>`, the REAL kernel. Its declared ninja output is the `.util` stamp, so
  the file it actually writes is an **undeclared output**.

Ninja therefore believes the unbuildable link owns that path, and the copy
writes it behind ninja's back. `add_dependencies` makes `<exe>_build` run
FIRST — it does not stop the link from running afterwards and clobbering the
result.

## Blast radius: dormant by default, live by name

* A plain `ninja` is SAFE — `ninja -n` shows only `Copying <exe> to build
  directory`. The link is `EXCLUDE_FROM_ALL`, so `all` never reaches it. This
  is why every fixture build passes.
* Asking for the target BY NAME triggers it: `cmake --build . --target <exe>`
  or `ninja <exe>`. That form is not hypothetical —
  `scripts/build/workspace-fixtures-build.sh:10` documents
  `cmake --build … --target <entry>` as one of its two build paths.

So the tree is one `--target` away from silently losing a fixture, and the
failure mode is a MISSING BINARY, which the test side reports as "not prebuilt"
— pointing at the fixture build rather than at the thing that deleted it.

## Fix, not applied here

The copy should be a `POST_BUILD` step of a target that can actually build, or
the carrier should declare the kernel as its `BYPRODUCTS`/`OUTPUT` so ninja
knows who owns the path. What must not remain is two rules writing one file
with only `add_dependencies` between them.

## Found while chasing issue 0870, and NOT yet shown to cause it

0870's nuttx C++ action cell fails intermittently. This defect is a plausible
mechanism — a deleted or stale image explains a lot — but it is **not
demonstrated** to be the cause: normal fixture builds never invoke the
destructive path, and the 0870 flake has not reproduced in 17+ consecutive runs
since. Recorded as its own bug because it is one regardless of 0870.

Found by following the "do not `rm -rf`, find the edge" practice: `ninja -t
query <exe>` showed `<exe>_build` as an ORDER-ONLY (`||`) dependency, which is
what prompted looking at who really writes the file.

## Fixed

Two changes that only work together:

* **`BYPRODUCTS` on the copy** (`packages/api/nros-c/cmake/nros-nuttx.cmake`) —
  names the real producer of `<build>/<name>`, so ninja stops attributing that
  path to the carrier.
* **`OUTPUT_NAME "<name>.carrier-do-not-run"` on the carrier** (BOTH board
  modules) — moves the unbuildable link off the kernel's path.

Verified against the command that destroyed the kernel, on both ports:

    cpp_action_client   rc=0   SURVIVED
    c_talker            rc=0   SURVIVED

`rc=0`, not merely a harmless failure: with the paths separated and the owner
declared, ninja no longer needs the carrier's link at all, so the invocation now
does the right thing (`[1/1] Copying <name> to build directory`). The image still
runs — `test_rtos_action_e2e` nuttx/C++ passes in 22.6 s.

The two halves are now SELF-ENFORCING rather than a convention: `BYPRODUCTS`
lives in the shared helper both ports use, so a port that declares it without
renaming its carrier fails the configure outright with `multiple rules generate
<name>`. That is exactly how the RISC-V port was found — fixing only ARM turned
its silent collision into a loud one.

## Migration hazard for anyone with an existing build dir

Once ninja fails at LOAD with `multiple rules generate`, it cannot re-run cmake
to regenerate itself — the error happens before any rule executes. A stale
`build.ninja` from a half-updated tree is therefore a dead end that does not
self-heal.

The escape is a re-configure, NOT a wipe:

    cmake <build-dir>          # re-runs configure from the cached settings

Measured here: `re-configure rc=0`, 4 carrier renames, 0 collisions, and the
full `just nuttx build-fixtures` then returned 0.

## Acceptance

* `cmake --build . --target <exe>` either produces a working kernel or fails
  without destroying the existing one.
* One rule owns `build-zenoh/<exe>`, and ninja knows which.
