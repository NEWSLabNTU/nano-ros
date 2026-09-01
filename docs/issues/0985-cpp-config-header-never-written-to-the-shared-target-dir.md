---
id: 985
title: "The C++ side never writes `nros_config_generated.h` to the shared target
  dir, so issue 0978's mirror falls back to the leaf copy every time — and the
  leaf copy is a museum"
status: open
type: bug
area: cmake, build
severity: high
found: 2026-09-02
related: [issue-0978, issue-0805, issue-0834, issue-0369, issue-0088]
---

## Symptom

`just build native` fails linking `c_listener` (`fixture-linux-c-zenoh`), with
issue 0369's size anchor and the SAME hash issue 0978 was filed for:

```
/usr/bin/ld: CMakeFiles/c_listener.dir/src/main.c.o:(.data.rel.ro+0x0):
  undefined reference to `nros_config_variant_sz_f3c40eb64e98fb7d'
/usr/bin/ld: libstd_msgs__nano_ros_c.a(std_msgs_msg_string.c.o):(.data.rel.ro+0x0):
  undefined reference to `nros_config_variant_sz_f3c40eb64e98fb7d'
```

**This is not 0978 regressing.** 0978's fix is present and working — it is
visible working in the measurement below. This is the case its fallback arm
cannot help with.

## Measured

`examples/native/c/listener/build-zenoh` holds FOUR copies of
`nros_config_generated.h` with THREE different size anchors:

| copy | anchor | mtime |
| --- | --- | --- |
| `nros-c/include/nros/…` | `sz_9a3e918900c9d46d` | **2026-09-02 03:12:42** |
| `nros-cpp/include/nros/…` | `sz_f3c40eb64e98fb7d` | **2026-09-02 03:12:43** |
| `nros-c/…` | `sz_cd6bc387c5d734f9` | 2026-09-01 04:02 |
| `nros-cpp/…` | `sz_f3c40eb64e98fb7d` | 2026-08-26 20:22 |

Both `include/nros/` copies were written **by the same build, one second
apart**. The C one is current; the C++ one carries the 2026-08-26 generation.
The archives and every shared copy agree on the current `sz_9a3e918900c9d46d`:

```
libnros_c.a      nros_config_variant_sz_9a3e918900c9d46d
libnros_cpp.a    nros_config_variant_sz_9a3e918900c9d46d
build/corrosion-cargo/native/*/nros-c-generated/nros/nros_config_generated.h   sz_9a3e918900c9d46d
```

## Root cause: the C++ side has no shared copy of THAT file to prefer

Issue 0978 made `mirror-generated-header.sh` prefer the leaf-independent copy at
`$CARGO_TARGET_DIR/nros-{c,cpp}-generated/nros/<name>` and fall back to the
leaf's own corrosion dir. For `nros-c` the shared copy exists, so the mirror
takes it and the C header is current — 0978 working.

For `nros-cpp` it does not exist. Every native shared dir holds:

```
$ ls build/corrosion-cargo/native/*/nano-ros_1147c/nros-cpp-generated/nros/
nros_cpp_config_generated.h
nros_cpp_config_generated.h.stamp
```

`nros_cpp_config_generated.h` — a DIFFERENT file — and no
`nros_config_generated.h`. So candidate (2) is absent for this name on the C++
side, the fallback fires **every time by construction**, and it resolves to the
leaf's own copy, which nothing has rewritten since 2026-08-26.

0978's premise was "the shared copy is refreshed by ANY leaf's run, so it is
never staler". True — where it is written at all. `write_header_to_target_dir`
writes the C++ package's OWN header there and not the shared
`nros_config_generated.h`, so for that one name the guarantee does not hold and
the fallback is not a fallback: it is the only arm.

## Not issue 0834

0834's signature is "a `.stamp` with no `.h` beside it", absorbing, repairable
only by `rm -rf`. Here the stamp's own header IS present; a *different* header
is missing, and the state is explainable and fixable without a wipe. Worth
separating, because the directory listing looks superficially like 0834's.

## Why the C link picks up the C++ copy

`main.c.o` and `std_msgs_msg_string.c.o` both reference `sz_f3c40eb64e98fb7d`,
which is only in the two `nros-cpp` copies — so `c_listener`'s include path
reaches `nros-cpp/include/nros/` for this header. Whether that is intended
(nros-c's public headers include nros-cpp's) or an include-order accident is NOT
yet established and should be answered before fixing: if a C target should never
resolve this header from the C++ package, that ordering is a second defect
sitting behind this one.

## Direction

1. Establish whether `nros_config_generated.h` should be written to the shared
   target dir by the C++ side too, or whether the C++ mirror should read the C
   side's shared copy (they are the same generated content — the sizes header —
   which is what makes four copies possible at all).
2. Answer the include-path question above before choosing, since it decides
   whether the C++ copy needs to be current or needs to not be reachable.
3. Whichever: the invariant worth gating is that **no two
   `nros_config_generated.h` under one build dir may disagree**. That is
   checkable directly, cheaply, and would have caught 0088, 0114, 0122, 0123,
   0245, 0268, 0978 and this — the whole family — at the point of divergence
   rather than as a link error naming a hash.

## How it was found

Three deep. Issue 0978 unblocked the C/C++ link stage; issue 0979 unblocked the
Rust build-script stage; issue 0984 unblocked the Rust link stage; this is what
the lane reached next. Each was invisible until the one in front of it was
fixed.

## Acceptance

* `just build native` links `fixture-linux-c-zenoh`.
* No two copies of `nros_config_generated.h` under one build dir disagree, and
  a gate says so.
