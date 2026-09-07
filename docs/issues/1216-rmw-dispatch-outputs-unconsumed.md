---
id: 1216
title: "Three of the eight `nros_rmw_dispatch` outputs are set and read by nothing — including the one that carries a backend's link libraries"
status: open
area: cmake, rmw, build
severity: medium
found: 2026-09-08
related: [1214, 1215, 0475, 0837, RFC-0071]
---

# A declared link channel that is inert, and whose value encodes the approach that breaks the link

`cmake/NanoRosRmwDispatch.cmake:5-13` documents eight variables
`nros_rmw_dispatch(<rmw>)` sets in the caller's scope. Measured consumer sites,
repo-wide, excluding the generated file itself and its generator
(`packages/cli/cargo-nano-ros/src/rmw_resolver.rs`), and excluding
`third-party/`:

| variable | consumer sites |
| --- | ---: |
| `NROS_RMW_PER_MESSAGE_HOOK` | 3 |
| `NROS_RMW_NEEDS_CXX_LINKER` | 3 |
| `NROS_RMW_CPP_DEFINE` | 2 |
| `NROS_RMW_CMAKE_TARGET` | 2 |
| `NROS_RMW_CAPABILITIES` | 1 |
| **`NROS_RMW_EXTRA_LINK_LIBS`** | **0** |
| **`NROS_RMW_RLIB_DEP`** | **0** |
| **`NROS_RMW_UMBRELLA_CFFI_FEATURE`** | **0** |

Command used:

```sh
for v in NROS_RMW_EXTRA_LINK_LIBS NROS_RMW_RLIB_DEP NROS_RMW_UMBRELLA_CFFI_FEATURE \
         NROS_RMW_CMAKE_TARGET NROS_RMW_CAPABILITIES NROS_RMW_PER_MESSAGE_HOOK \
         NROS_RMW_NEEDS_CXX_LINKER NROS_RMW_CPP_DEFINE; do
  n=$(grep -rn "$v" cmake packages examples integrations zephyr \
      | grep -v third-party | grep -v NanoRosRmwDispatch.cmake \
      | grep -v rmw_resolver.rs | grep -c .)
  echo "$v: $n"
done
```

(`NROS_RMW_CMAKE_TARGET`'s two sites are one live guard that never fires — see
issue 1215.)

## The dangerous one

`NROS_RMW_EXTRA_LINK_LIBS` is documented at `NanoRosRmwDispatch.cmake:8` as
"a `;`-list of extra link libs (cyclonedds C++ path)", and its cyclonedds value
is `"nros_rmw_cyclonedds;ddsc;stdc++"` (`:18`). It is a plausible-looking name
holding a plausible-looking value, and consuming it the obvious way —
`target_link_libraries(${TARGET} ${NROS_RMW_EXTRA_LINK_LIBS})` — is precisely
the approach issue 0475 records as **breaking the link**:

> adding `target_link_libraries(NanoRos INTERFACE nros_rmw_cyclonedds)` beside
> the flag breaks the link with `undefined reference to
> ddsrt_malloc/calloc/free`
> — `docs/issues/archived/0475-fixture-stale-probe-and-build-graph-disagree.md:104-113`

The same warning is repeated at `cmake/NanoRosSupportLibrary.cmake:481-487`. The
real cyclonedds link is a raw whole-archive flag built at `CMakeLists.txt:305-307`
and `:340-341`, with the file edge supplied separately by `LINK_DEPENDS` at
`cmake/NanoRosLink.cmake:101-104`.

So the variable is not merely dead: it is a maintained, generated, documented
declaration of how to link a backend, whose only correct use is *not to use it*.
0475's own investigation notes flagged it as a lead
(`0475-...md:62-74`); the fix landed via `LINK_DEPENDS` and the variable was
never removed.

## Why the whole-archive question does not currently generalise

The one thing that *does* generalise is the rebuild edge:
`cmake/NanoRosLink.cmake:101-104`

```cmake
if(TARGET nros_rmw_${_chosen})
    set_property(TARGET ${TARGET} APPEND PROPERTY
        LINK_DEPENDS "$<TARGET_FILE:nros_rmw_${_chosen}>")
endif()
```

This is keyed on a **naming convention** — a provider whose cmake target is
literally `nros_rmw_<name>` gets the file edge for free. That is a real,
working generalisation of 0475's fix, and nothing documents it as a contract or
gates it. A provider that names its target anything else silently gets the
0475 defect back (build order without a file edge → museum binaries).

The force-link half does not generalise at all. zenoh and xrce are bundled into
`libnros_c.a`/`libnros_cpp.a` and anchored by `#[used]` statics, so they need
none; cyclonedds gets `-Wl,--whole-archive` from a hand-written arm at
`CMakeLists.txt:293-344`. There is no path by which a provider's declaration
produces a whole-archive group.

## Fix direction (not applied)

Either delete the three unconsumed variables (and the `RmwDispatch` fields that
generate them, in `rmw_resolver.rs:189/205/214`), or wire them — but if wired,
`EXTRA_LINK_LIBS` must not become a `target_link_libraries` call. Document the
`nros_rmw_<name>` target-naming convention that `LINK_DEPENDS` depends on, and
gate it, in the spirit of `just check cmake-support-library`
(`just/check/cmake.just:120-141`), which already does this for the support-library
class. Note there is no gate at all on the RMW link path today; verification is
the manual `ninja -t query` recipe in the comments at
`cmake/NanoRosLink.cmake:99-100`.
