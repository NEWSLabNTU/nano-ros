---
id: 603
title: "`nros setup --system` reports libmbedtls present when only the RUNTIME
  package is installed, so the build dies 20 minutes later on missing headers"
status: resolved
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

There was no probe kind that could express "the headers are installed".
`sharedlib` answers about the runtime, and `pkg_config` — documented as the dev
probe — cannot answer for mbedTLS at all, because Ubuntu's `libmbedtls-dev`
ships no `.pc` file. That absence is precisely why
`generate_mbedtls_pc_files` has to fabricate one, so reaching for `pkg_config`
here would have re-entered the loop that made issue 0399.

So `CheckProbe` gains a `header` kind: the include spelling
(`mbedtls/entropy.h`, not a path) checked against `/usr/include`,
`/usr/local/include` and the Debian multiarch dir. Linux-only and `unknown`
elsewhere, matching `sharedlib`'s existing contract. Written as the include
spelling so the probe and the `#include` that needs it are the same string —
`libmbedtls` now names the exact header `runner.rs` checks before fabricating
the `.pc` files, so the gate and the build ask one question.

No compiler is invoked: this runs for every entry on every `nros setup`, and a
probe that shells a compiler would cost more than the check is worth. A package
installing outside those roots should use `pkg_config` or `sharedlib` instead.

Verified on the host that produced the bug:

```
$ nros setup --system --check
17 present, 5 missing, 2 unprobed
  [MISSING] libmbedtls — zenoh-pico TLS link (zpico-sys)
Error: 5 system package(s) missing. Install with:
  sudo apt-get install -y … libmbedtls-dev
```

`libz3` still reports present here (its header is installed), confirming the new
probe does not simply answer "missing" — which would trade this false positive
for a false negative and hard-block setup on a provisioned host. A unit test
pins both directions.

**Swept — and the sweep found the rule is not "every `-dev` entry":**

| entry | apt package | probe | verdict |
| --- | --- | --- | --- |
| `libmbedtls` | `libmbedtls-dev` | was `sharedlib` | **wrong** — fixed, `header = "mbedtls/entropy.h"` |
| `libz3` | `libz3-dev` | was `sharedlib` | **wrong** — fixed, `header = "z3.h"` |
| `libclang-dev` | `libclang-dev` | `sharedlib = "libclang"` | **correct — deliberately unchanged** |
| `libslirp` | `libslirp0` | `sharedlib = "libslirp.so.0"` | correct — runtime package, versioned SONAME |

The rule is what the CONSUMER needs, not what the package is named.

`libclang-dev` looked like the same defect and is not, which is why it was
checked rather than swept blindly. bindgen `dlopen`s libclang at run time — it
wants the loadable library, not `/usr/include/clang-c`. And this very host
proves a header probe there would be a false negative: `libclang-12-dev` and
`libclang-14-dev` are both installed and working, while
`/usr/include/clang-c/Index.h` does **not** exist, because versioned dev
packages install under `/usr/lib/llvm-14/include`. Swapping its probe would have
traded a false positive for a false negative and hard-blocked `nros setup` on a
correctly provisioned machine.

`libz3` had no observed failure — it was caught by the sweep. Its dev package is
installed here, so the old probe happened to be right by accident; on a host
with only `libz3-4` it would have read present and z3-sys would then have failed
to find `z3.h`.

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
