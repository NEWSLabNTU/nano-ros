---
id: 1224
title: "`nros-rmw-cyclonedds` declares `<build_type>nros_cmake</build_type>` while cargo is what builds it, and RFC-0094 W3 makes that declaration load-bearing"
status: open
area: build, rmw, cli
severity: medium
found: 2026-09-08
related: [1207, RFC-0094, RFC-0087]
---

# The one row in the routing diff that is neither a fix nor a no-op

Phase-439 W0 pushed all 416 tracked `package.xml` through RFC-0094 D3's rule —
`<build_type>` selects the DRIVER, file presence selects PARTICIPATION — and
diffed it against the three file-presence sites that route today. 21 packages
change side, and D3 predicted that class: a package carrying both a
`CMakeLists.txt` and a `Cargo.toml` and declaring a cmake build type leaves the
generated `[workspace] members` list, because cmake drives it.

For 20 of them that is demonstrably safe. Each carries an independent reason
cargo membership is impossible or already waived:

* **13 declare their own `[workspace]` table**, so listing them as a member of
  another root is a hard cargo error — measured against a synthetic
  two-manifest tree on this host:

      error: multiple workspace roots found in the same workspace:
        …/src/leaf
        …

* **13 declare `[package.metadata.nros.entry] deploy`**, which
  `cargo_excluded_entry_dirs` (`packages/cli/nros-cli-core/src/cmd/build.rs`)
  already resolves through the board catalog to `Driver::West` and excludes.

(Six carry both.) So for those, D3 reaches today's answer from the declaration
instead of a `Cargo.toml` metadata round-trip through the board catalog.

**`packages/rmw/cyclonedds/nros-rmw-cyclonedds` carries neither.**

## What is actually true of it

* It is member 87 of the repo-root `[workspace]` in `/Cargo.toml`, and it is a
  real `rlib` that the cyclone backend graph depends on. **Cargo builds it.**
* Its `CMakeLists.txt` is `project(nros_rmw_cyclonedds … LANGUAGES C CXX)` — a
  separate C/C++ wrapper with its own RPATH handling for the in-tree
  `libddsc.so`, built for the backend's own test binaries. It is not the thing
  that produces the crate.
* Its `package.xml` declares `<build_type>nros_cmake</build_type>`, which under
  RFC-0094 D3 says *cmake drives this package*.

The declaration is false about the Rust half, and it is the only in-tree
dual-file package where the file evidence and the declaration disagree with
nothing else to break the tie.

## Why it does not break anything today, and why that is the problem

The three routing sites operate on one workspace's discovery walk, and no nano-ros
workspace walk reaches `packages/rmw/`. So the wrong declaration is unread — which
is exactly RFC-0094's complaint, stated one package over: *a declaration exists, is
authored, is gated for correctness, and something else is load-bearing.*

RFC-0094 W3 makes `<build_type>` load-bearing at all three sites. The day it lands,
any workspace whose walk reaches this package drops it from the cargo members list
on the strength of a declaration that is wrong. Nothing in the tree does that today;
nothing stops it either.

## The fix is the declaration, not the rule

Two candidates, and the choice is a decision W3 has to make rather than something
the gate can pick:

1. **`<build_type>nros_cargo</build_type>`** — the honest answer for the package
   as it stands. Cargo builds the crate; the `CMakeLists.txt` is a sibling test
   project, in the same relationship as any crate with a C harness beside it.
2. **Split the CMake wrapper out** into its own directory with its own
   `package.xml`, so one directory is one buildable thing. That is the shape D3
   assumes and the reason the ambiguity exists here at all.

Do NOT widen D3's rule to accommodate it. The rule's value is that it has exactly
one exception class and that class is justified; a second class carved to fit one
package is how a routing rule stops being checkable.

## Reproduce

    python3 scripts/check/check-package-routing.py --report

The row prints as `NEITHER — read this one` under "Packages that CHANGE SIDE".
The gate is green today (the routing SHAPE is D3's named class); it is the
evidence column, not the verdict, that names this package.
