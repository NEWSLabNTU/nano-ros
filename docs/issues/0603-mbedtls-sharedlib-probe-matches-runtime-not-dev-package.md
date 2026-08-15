---
id: 603
title: "`nros setup --system` reports libmbedtls present when only the RUNTIME
  package is installed, so the build dies 20 minutes later on missing headers"
status: open
type: bug
area: build
related: [issue-0399, issue-0466, issue-0368, issue-0196, rfc-0062]
---

## Symptom

`just build-test-fixtures lane=tier2` on this Ubuntu host built **every embedded
family** — zephyr, threadx-linux, threadx-riscv64, freertos, qemu, esp32, all OK
— then died ~20 minutes in, in the `native` leaf lane:

```
thread 'main' panicked at nros-zpico-build/src/runner.rs:1719:9:
TLS is enabled but mbedTLS headers are missing (/usr/include/mbedtls/entropy.h).
    Debian/Ubuntu:  sudo apt-get install libmbedtls-dev
make[1]: *** [.../linux-rust-all-…mk:100: fixture-0030] Error 101
```

The prerequisite IS declared, and the gate that exists to catch exactly this
says everything is fine:

```
$ nros setup --system --check
nros setup --system --check: 18 present, 4 missing, 2 unprobed
  [MISSING] gcc-riscv64-unknown-elf …
  [MISSING] genromfs …
  [MISSING] kconfig-frontends …
  [MISSING] libgcrypt-dev …
```

`libmbedtls` is not in that list. The probe reports it **present**.

## Mechanism

`nros-sdk-index.toml` declares it correctly:

```toml
[system.libmbedtls]
why = "zenoh-pico TLS link (zpico-sys)"
apt = ["libmbedtls-dev"]
check = { sharedlib = "libmbedtls.so" }
```

The `apt` mapping names the **dev** package. The probe looks for a **shared
library**. Those are different packages, and this host has exactly the split
that separates them:

```
$ dpkg -l | grep mbedtls
ii  libmbedtls14:arm64  2.28.0-1build1   lightweight crypto and SSL/TLS library - tls library

$ ldconfig -p | grep mbedtls
    libmbedtls.so.14 (libc6,AArch64) => /lib/aarch64-linux-gnu/libmbedtls.so.14

$ ls /usr/include/mbedtls/entropy.h
(absent)
```

`libmbedtls14` — the RUNTIME package — ships the versioned SONAME
`libmbedtls.so.14`. The probe's `sharedlib = "libmbedtls.so"` matches it. The
unversioned `libmbedtls.so` symlink and the `mbedtls/*.h` headers both come from
`libmbedtls-dev`, which is not installed. So the probe answers a question
(is the runtime library loadable?) that is not the question the build asks
(can I compile against the headers?).

Any host with a TLS-using package pulled in as a dependency has the runtime and
not the dev headers, so this is the common case, not an exotic one.

This is the issue-0196 rule: a gate whose coverage is narrower than the rule it
enforces. It is also why the failure reads as a broken tree rather than an
unmet precondition — the surface designed to say "install this first" already
said "you have it".

## Fix

The probe must test what the consumer needs. For a `-dev` mapping that is a
header, not a SONAME:

```toml
check = { header = "mbedtls/entropy.h" }
```

if a `header` probe kind exists, or a `sharedlib` match that requires the
unversioned symlink rather than any versioned SONAME. The build script already
names the exact path it needs (`runner.rs:1719` checks
`/usr/include/mbedtls/entropy.h`), so the gate and the build can share that one
predicate instead of spelling it two different ways — the recurring rule in this
repo.

**Three entries share the defect and one does not — swept:**

| entry | apt package | probe | verdict |
| --- | --- | --- | --- |
| `libmbedtls` | `libmbedtls-dev` | `sharedlib = "libmbedtls.so"` | **wrong** — dev pkg, runtime probe |
| `libclang-dev` | `libclang-dev` | `sharedlib = "libclang"` | **wrong** — same shape, and the loosest pattern of the three |
| `libz3` | `libz3-dev` | `sharedlib = "libz3.so"` | **wrong** — same shape |
| `libslirp` | `libslirp0` | `sharedlib = "libslirp.so.0"` | correct — RUNTIME package, versioned SONAME |

`libslirp` is the control that shows the rule: a `sharedlib` probe is right when
the mapped package IS the runtime library, and wrong when it is the `-dev`
package, because the dev package's distinguishing contents are headers and the
unversioned symlink. Fixing only the reported site is the class-recurrence
pattern CLAUDE.md warns about, so all three move together.

Sweep:

```sh
grep -n -B6 'sharedlib' nros-sdk-index.toml | grep -E 'apt = |sharedlib'
```

## Impact

Cheap to work around once known (`sudo apt-get install -y libmbedtls-dev`) but
expensive to discover: the sweep spends ~20 minutes building six embedded
families before reaching the row that fails, and the one command a user would
run first to avoid that reports success.

## Notes

The panic itself is correct and should stay. It is the issue-0399 fix: the `.pc`
files `pkg_check_modules` reads are FABRICATED by `generate_mbedtls_pc_files`
with `includedir=/usr/include` hardcoded, so `pkg_config::probe("mbedtls")`
reads back what nano-ros just wrote and cannot fail. Without the header check
the build sails past discovery and dies ~40 lines later inside a vendored TU,
reading as a broken vendored tree. This issue is about the gate upstream of it,
not that guard.

Do not be misled by `packages/rmw/zenoh/zpico-sys/mbedtls` (v2.28.9, a real
submodule): that vendored tree is cross-compiled for embedded zenoh-pico. The
native host TLS link uses the system library. Both are correct; nothing says so
at the point of failure.
