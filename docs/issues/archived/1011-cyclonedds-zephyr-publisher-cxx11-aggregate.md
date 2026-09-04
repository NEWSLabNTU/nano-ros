---
id: 1011
title: "`publisher.cpp` brace-initializes a struct with default member
  initializers, which is not an aggregate under the Zephyr lane's `-std=c++11`"
status: resolved
type: bug
area: rmw-cyclonedds, zephyr
severity: medium
related: [issue-0998]
found: 2026-09-03
---

## Measured

All six `rust-*-cyclonedds` zephyr fixtures fail to build, identically:

```
FAILED: modules/nros/CMakeFiles/nros.dir/.../nros-rmw-cyclonedds/src/publisher.cpp.obj
publisher.cpp:287:37: error: no matching function for call to
  'nros_rmw_cyclonedds::NrosCdrBlob::NrosCdrBlob(<brace-enclosed initializer list>)'
```

The site (`publisher.cpp:287`):

```cpp
const NrosCdrBlob blob{data, len};
```

and the type (`nros_sertype.hpp:54`):

```cpp
struct NrosCdrBlob {
    const uint8_t* data{nullptr};
    size_t size{0};
};
```

## Cause

The Zephyr lane compiles this TU with **`-std=c++11`** (visible in the failing
command line, alongside `-fno-exceptions -fno-rtti -nostdinc++`).

A class with default member initializers is **not an aggregate in C++11** —
C++14 relaxed exactly this rule. So `NrosCdrBlob{data, len}` is not aggregate
initialization there; the compiler looks for a two-argument constructor and
finds none. Every other lane builds this file at C++14 or later and is fine,
which is why it is invisible outside Zephyr.

## Not a regression from issue 0998, but unmasked by it

Note the build order in the same log:

```
[1221/1292] Building CXX object .../nros_sertype.cpp.obj      <- OK
[1222/1292] Building CXX object .../publisher.cpp.obj         <- FAILED
```

0998's fix works: the sertype TU now compiles on a freestanding board. The
build then reaches the NEXT TU in the same module, which has an independent
C++11 defect. `publisher.cpp`, `nros_sertype.hpp` and the cmake are all
untouched by that branch, so this TU's inputs are byte-identical to main — the
failure predates it and was simply hidden behind an earlier one. CLAUDE.md's
"one fix can unmask the next", in the build rather than in CI.

## Fix options

Least invasive, and C++11-clean at both existing use sites:

```cpp
struct NrosCdrBlob {
    const uint8_t* data{nullptr};
    size_t size{0};
    NrosCdrBlob() = default;
    NrosCdrBlob(const uint8_t* d, size_t s) : data(d), size(s) {}
};
```

Check before taking it: the sertype code does `std::memset(samples, 0,
sizeof(NrosCdrBlob) * count)` and `sizeof(NrosCdrBlob)`, so whether a
user-provided constructor disturbs any triviality assumption in that path needs
reading, not assuming.

Raising the Zephyr lane to `-std=c++14` is the other direction and a much wider
blast radius (Zephyr's minimal libcpp), so it should not be done just for this.

## Acceptance

* [ ] The six `rust-*-cyclonedds` zephyr fixtures build.
* [ ] No new assumption broken in the sertype sample path.


## Resolved 2026-09-04 — fixed and gated, the issue just outlived its own fix

The fix landed in PR #341 and is on main: `NrosCdrBlob` no longer carries default
member initialisers, so it is a C++11 aggregate again and
`NrosCdrBlob{data, len}` compiles under the Zephyr lane's `-std=c++11`.

    struct NrosCdrBlob {
        const uint8_t* data;
        size_t size;
    };

Dropping the initialisers is also the better type: no site default-constructs
one — all five are brace-init or a `static_cast` from `void*` — so they bought
nothing, and removing them restores trivial DEFAULT CONSTRUCTIBILITY on top of
trivial copyability. `sertype_zero_samples` memsets these and
`sertype_realloc_samples` hands them to `dds_realloc`; that code's comment
already called the sample "a trivially copyable two-word struct", which was only
half true before.

Gated by `tests/sertype_sample_cxx11.cpp`, the ONLY C++11 translation unit in
the tree — nothing else in `just check` compiles this header at that standard,
which is how the defect survived. Non-vacuous by mutation: restoring the
initialisers fails with both the triviality `static_assert` and the production
error verbatim.

**Why this row stayed open after its own fix merged.** The fix and the issue
travelled on different branches: #341 carried the code, while the issue file was
still on the unmerged #318. #341 therefore could not archive it, and nothing
noticed once #318 landed. Worth knowing as a shape — an issue can outlive its
fix by exactly the distance between two PRs — because the open list is what
people read to decide what is left.
