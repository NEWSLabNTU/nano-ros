---
id: 1213
title: "`probe_net_type_sizes` publishes its result through two carriers aimed at
  `zpico-platform-shim`, a crate Phase 129.D deleted — and its doc comment states
  the opposite of what the code 280 lines below it says"
status: open
type: bug
area: rmw, build
severity: low
found: 2026-09-08
related: [issue-0135, issue-0207, issue-0963, phase-451]
---

## Problem

`probe_net_type_sizes` (`packages/rmw/zenoh/nros-zpico-build/src/runner.rs:1649`)
compiles a C size probe and recovers `sizeof(_z_sys_net_socket_t)` /
`sizeof(_z_sys_net_endpoint_t)` from the resulting archive. It then publishes the
two numbers through **three** carriers. Only one of them has a reader.

```rust
// runner.rs:1924-1938
// Emit as DEP variables (available to direct dependent crates as DEP_ZPICO_*)
println!("cargo:SOCKET_SIZE={}", socket_size);          // (1) dead
println!("cargo:ENDPOINT_SIZE={}", endpoint_size);      // (1) dead

// Also emit as rustc-env so zpico-platform-shim can read them.
// zpico-platform-shim is a dependency of zpico-sys (not the other way),
// so DEP variables don't flow. Instead, write a shared file.
let sizes_file = out_dir.join("net_type_sizes.txt");    // (3) LIVE
std::fs::write(&sizes_file, ...).unwrap();
// Export the path so zpico-platform-shim's build.rs can find it
println!("cargo:rustc-env=ZPICO_NET_SIZES_FILE={}", ...); // (2) dead
```

**Carrier (3), the file, is live and correct.** It is read back in the same build
script at `runner.rs:1542` and lowered into `NROS_ZP_VENDOR_NET_SOCKET_SIZE` /
`NROS_ZP_VENDOR_NET_ENDPOINT_SIZE` (`:1551-1552`), which are the operands of the
`_Static_assert` drift guards in
`packages/rmw/zenoh/zpico-sys/c/zpico/platform_aliases.c:454-461`. That is the
issue-0135 mismatched-TU guard and it must stay.

**Carriers (1) and (2) have no consumer anywhere in the tree.** Measured
2026-09-08 over every `*.rs` / `*.toml` outside `third-party/`, `build/` and
`target*/`:

* `DEP_ZPICO_SOCKET_SIZE` / `DEP_ZPICO_ENDPOINT_SIZE` — **0 readers**. The only
  two hits are the comment at `runner.rs:1647` and the comment at `:1924`.
* `ZPICO_NET_SIZES_FILE` — **0 readers**. No `env!("ZPICO_NET_SIZES_FILE")`
  exists; the only hit is the `println!` that emits it.

The named consumer does not exist either: `zpico-platform-shim` was **retired in
Phase 129.D**, recorded in eleven places (`Cargo.toml:149`,
`packages/rmw/zenoh/zpico-sys/Cargo.toml:117`,
`packages/rmw/zenoh/zpico-sys/src/lib.rs:28`,
`packages/rmw/zenoh/zpico-link-ivc/Cargo.toml:6` — "split out of
zpico-platform-shim" — and others). `ls packages/rmw/zenoh/` confirms no such
directory.

## The doc comment is worse than the dead code

`runner.rs:1642-1647`:

```
/// reads the symbol sizes from the resulting .o file, and emits them as
/// `cargo:SOCKET_SIZE=<N>` and `cargo:ENDPOINT_SIZE=<N>` DEP variables.
/// zpico-platform-shim reads these as `DEP_ZPICO_SOCKET_SIZE` / `DEP_ZPICO_ENDPOINT_SIZE`.
```

The code at `:1928-1930`, 280 lines below, states the exact contradiction:

```
// zpico-platform-shim is a dependency of zpico-sys (not the other way),
// so DEP variables don't flow.
```

Both cannot be true, and the second is the correct one — `DEP_<LINKS>_<KEY>`
flows from a `links` package to its **dependents**, never to its dependencies.
A reader who trusts the docblock will believe there is a working cross-crate
number channel here and reach for it; there is not, and the direction it names
is structurally impossible.

This is the same class as issue 0963 (an exported bound inventory with no
consumer), one crate over.

## Cost

Small but non-zero, and it is in the mechanism under study:

* two `cargo:` metadata keys are computed and published on every `zpico-sys`
  build, for nobody;
* `cargo:rustc-env=ZPICO_NET_SIZES_FILE=<absolute OUT_DIR path>` puts an
  absolute path into `zpico-sys`'s own rustc environment for no reader. NOT
  measured here, so stated as a suspicion rather than a finding: a `rustc-env`
  value participates in the crate's fingerprint, and an absolute build-directory
  path in a fingerprint is the issue-0491 class. Whether it actually costs a
  rebuild under the shared `build/cargo-fixtures/<slug>` dirs should be measured
  before it is claimed;
* the docblock actively misinforms about which direction `DEP_*` travels.

## Suggested fix

Delete carriers (1) and (2); keep the file. Rewrite the docblock to say what is
actually true: the sizes are recovered here and consumed **in this same build
script** at `:1542` as `cc::Build` defines for `platform_aliases.c`'s
`_Static_assert` guards, and there is no cross-crate channel because there is no
longer a crate on the other end.

Do **not** replace the dead carriers with a working one without a named
consumer — that is the shape issue 0963 names.

## How this was found

Build-system interop survey, 2026-09-08: an inventory of every `links = "..."`
declaration in the tree and, for each, its `cargo:<key>=` producers and its
`DEP_<LINKS>_<KEY>` readers. Six manifests declare `links`; `zpico` is the one
whose entire published surface is unread.

Reproduce:

```bash
grep -rn 'DEP_ZPICO' --include='*.rs' --include='*.toml' . \
  | grep -v third-party | grep -v '/build/' | grep -v '/target'
grep -rn 'ZPICO_NET_SIZES_FILE' --include='*.rs' . \
  | grep -v third-party | grep -v '/build/'
ls packages/rmw/zenoh/          # no zpico-platform-shim
```
