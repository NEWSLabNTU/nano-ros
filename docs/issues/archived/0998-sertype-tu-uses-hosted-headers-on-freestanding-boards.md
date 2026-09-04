---
id: 998
title: "`nros_sertype.cpp` includes `<memory>` and `<string>`, which no
  freestanding board has — the Cyclone backend stopped cross-building on
  2026-08-31 and nothing ran to notice"
status: resolved
type: bug
area: rmw, boards, build
severity: high
found: 2026-09-03
related: [issue-0968, issue-0970, issue-0984, issue-0112, issue-0942]
---

## Symptom

`just build-test-fixtures lane=tier2`, the `threadx_riscv64` module:

```
[68/122] Building CXX object .../nros_rmw_cyclonedds.dir/src/nros_sertype.cpp.obj
FAILED: [code=1] ...
nros_sertype.cpp:23:10: fatal error: memory: No such file or directory
make[1]: *** [.../threadx-riscv64-c-cyclonedds-all-...mk:10: fixture-0000] Error 1
```

## Cause

The board compiles this target with

```
-ffreestanding -nostdinc++ -isystem .../nros-board-threadx-qemu-riscv64/cxx-compat
```

and that shim carries exactly the headers a freestanding C++ implementation
must provide, and no more:

```
cstdarg cstddef cstdint cstdio cstdlib cstring initializer_list new type_traits utility
```

`<memory>` and `<string>` are not in it and cannot be — both need an allocator
and a hosted library. `nros_sertype.cpp` included both, plus used
`std::unique_ptr` and a `std::string` member.

**Every sibling TU in the same target already lives inside that subset:**
`publisher.cpp`, `subscriber.cpp` and `sertype_min.cpp` use only
`cstdlib`/`cstdint`/`cstring`/`new`. This one did not, and it is the newest —
added to the cmake target by `b4858f941` (issue 0970) on 2026-08-31.

The freestanding hazard was already known IN THIS FILE: twenty lines above the
offending include sits a comment citing issue 0942, explaining why `<cstdio>`
had to become `<stdio.h>` because "a freestanding libstdc++ does the reverse".
The same reasoning stops one header short.

This is issue 0112's class — a std include reaching a target that has no hosted
library — and it is why CLAUDE.md says to gate such includes rather than assume
them.

## Why nobody noticed

The Cyclone backend has not cross-built since 2026-08-31. Nothing runs it:
tier 2 is where these fixtures are built, and issue 0968 records that nobody has
run tier 2 in a long time. This defect was found by trying to reproduce 0968 —
the backlog that issue predicts, arriving.

Issue 0984 is the same commit's other half, one target over: `nros_sertype.cpp`
was added to the cmake list and not to the cargo one, so the RUST link failed
while C/C++ kept working. Now the reverse — the cmake list builds it for a
target whose headers it does not fit. One file, two build paths, two different
failures, both from the same landing.

## Fix

Bring the TU inside the freestanding subset, as its siblings already are:

* `<memory>` / `<string>` includes dropped.
* `std::unique_ptr<NrosSerdata>` → a local `SerdataOwner`, doing only what the
  three uses needed (own-or-release around `new (std::nothrow)`, `get()`,
  `operator->`). Deliberately not a general smart pointer: the shim is the
  freestanding header set, not a partial libstdc++, and growing a local
  `<memory>` would be a second spelling of a standard header.
* `std::string type_name` → `const char*`. The descriptor's `m_typename` is a
  static string in generated code and Cyclone copies the name into its own
  `ddsi_sertype` during `ddsi_sertype_init_flags`, so nothing here needs to own
  it. `==` becomes `strcmp`, the FNV hash loop walks the pointer.

Verified with the board's REAL flags, not a reconstruction — rebuilt the exact
failing object in `examples/qemu-riscv64-threadx/c/talker/build-cyclonedds`:

```
$ ninja .../nros_rmw_cyclonedds.dir/src/nros_sertype.cpp.obj
object rc=0     (18000 bytes)
```

and the hosted path still builds: `just check rmw-cyclonedds` rc=0,
`just check cyclone-backend-sources` OK.

### Three mistakes on the way, kept because the third is the useful one

1. The guard class was inserted before `NrosSerdata` was declared.
2. `auto d = SerdataOwner(...)` is copy-initialisation, which under `-std=c++14`
   needs an accessible copy/move constructor — deleted here on purpose. Direct
   initialisation, `SerdataOwner d(...)`, is what the code wanted.
3. The guard had no `operator->`, which `std::unique_ptr` has and the call sites
   used.

None of the three was caught by reading. All three were caught by building the
real object with the real flags, which is the only oracle for a target this host
cannot otherwise exercise.

## Landed as issue 1014's spelling, not this one

The same defect was diagnosed independently and fixed on `main` under
[issue 1014] before this branch merged. Both fixes drop `<memory>` and
`<string>`; they differ in the two replacements, and `main`'s is what the tree
carries:

| | this branch | landed (issue 1014) |
| --- | --- | --- |
| owning pointer | `SerdataOwner`, `NrosSerdata`-specific | `OwnPtr<T>`, a template |
| type name | `const char* type_name` added to `NrosSertype` | no member; reads `ddsi_sertype::type_name`, Cyclone's own strdup'd copy |

The second difference is the substantive one. `ddsi_sertype_init_flags` already
copies the name into the base, so a member here is a second pointer to a string
the base object also holds. `NrosSertype` stays an empty derived struct.

The code change from this branch was therefore dropped in the rebase and the
landed version kept. Everything below still describes what was measured on the
board, and the two spellings are equivalent on the point this issue is about:
the TU compiles inside the freestanding subset either way.

The `-std=c++14` copy-initialisation trap in mistake 2 below is NOT specific to
this spelling -- the landed `OwnPtr` hit it too and is direct-initialised at all
three call sites for the same reason.

## A confound I introduced, stated so nobody mines it

The tier-2 build that found this was still RUNNING when I began editing the
file. `freertos` failed in that run against my half-finished intermediate — its
errors are `'p_' was not declared` and `cannot convert NrosSerdata* to const
SerdataOwner&`, which are mistakes 1 and 2 above, not a property of the tree.
`native` failed in the same window and has NOT been attributed.

Only `threadx_riscv64` is established as pre-existing: it failed on `<memory>`
before the first edit. `nuttx` failed at 11:39:48, also before the edit, but its
cause is a bare `make ... Error 1` and is NOT diagnosed here.

The tier-2 baseline therefore has to be re-measured on a clean tree before any
of it feeds issue 0968.

## Acceptance

* [x] `nros_sertype.cpp` compiles under `-ffreestanding -nostdinc++` against
      the board's cxx-compat shim.
* [x] The hosted Cyclone path still builds.
* [ ] A clean tier-2 build, with no edits in flight, to re-establish which
      modules genuinely fail — for issue 0968.
