---
id: 932
title: "The linux-arm64 arm-none-eabi-gcc dist gets neither the ncurses bundle
  nor the gdb Python, and its gdb fails EARLIER than x86_64's did"
status: resolved
type: bug
area: tooling
related: [0926, 0928, 0929]
---

## What is missing

`arm-none-eabi-gcc` 13.2-nros3 ships two fixes on **linux-x86_64** and neither
on **linux-arm64**, whose tarball is byte-identical to -nros2 (same sha256 —
that identity is how the release proved it touched only one host):

* **ncurses 5 bundling** (issue 0928). Skipped because the arm64 leg then failed
  on the NEXT library.
* **the gdb Python stdlib** (issue 0929). Skipped because the arm64 gdb needs
  more than a stdlib.

## Why arm64 is a different problem, not the same one deferred

The two hosts fail at different STAGES, and the arm64 one fails earlier:

| | linux-x86_64 | linux-arm64 |
| --- | --- | --- |
| Python | STATIC in gdb; no `libpython` in `NEEDED` | DYNAMIC — links `libpython3.8.so.1.0` |
| failure | interpreter init aborts (stdlib absent) | the LOADER fails; gdb never starts |
| fix | ship the stdlib, point `PYTHONHOME` | needs the `.so` too, and 22.04 packages no python3.8 |

Measured during the -nros2 build: `bundle: cannot resolve libpython3.8.so.1.0`.
The bundler refused, correctly — it fails rather than shipping a dist bundled
everywhere except the one library it could not resolve.

Also note the ncurses spelling differs by architecture: x86_64 links
`libncursesw.so.5`, arm64 links `libncurses.so.5`. A package list derived from
one host is wrong for the other by construction, which already cost one build.

## What it would take

The blocker is obtaining `libpython3.8.so.1.0` for arm64 on a 22.04 runner:

1. **deadsnakes PPA** — cheapest, but adds a third-party apt source to a release
   build, which is a supply-chain decision rather than a packaging one.
2. **Build CPython 3.8 in the job** — self-contained and slow, and it makes the
   toolchain repackage a source build, which it deliberately is not today.
3. **Take the `.so` from python.org's build** — no compiler needed, but their
   binaries are not distributed for Linux, so this may not exist.
4. **Accept and declare** — leave arm64 as-is and let `smoke` report `[BROKEN]`
   there, which is honest and costs nothing. `system = ["libcrypt1"]` already
   holds for both hosts.

Direction 4 is the current de-facto state and is not wrong; this issue exists so
it is a CHOICE rather than an oversight.

## Verifying any fix

Nothing here can be checked on an x86_64 developer host. The release job's own
`gdb-python: OK` assertion runs per host, so an arm64 fix proves itself in CI —
and `nros setup --tool arm-none-eabi-gcc --check` reports `[BROKEN]` on an arm64
host until it lands, via the `smoke` probe (phase-404 / issue 0929).

## Resolved (2026-08-30) — `arm-none-eabi-gcc` 13.2-nros4

Both fixes now land on arm64, and the route is DIFFERENT from x86_64's because
the hosts embed Python differently — which is exactly why one fix had not
covered both.

**The shared library comes from Ubuntu's own archive.** 22.04 packages no
python3.8; focal does, for arm64, at `ports.ubuntu.com`. That is the same
publisher as the runner's own packages — not a PPA (a supply-chain decision
nobody asked for) and not a source build (which would turn a repackage into a
compile). It needs at most GLIBC_2.29 against the runner's 2.35, so a
focal-built library runs on jammy.

Of the four directions this issue listed, the one taken was **none of them**:
direction 3 said python.org publishes no Linux binaries, which is true, and I
had not thought to ask whether UBUNTU publishes an older one. It does.

**Ordering was the thing that had failed before.** libpython goes into the
prefix and the rpath is set BEFORE `bundle_linux_libs`, because the bundler
resolves through `ldd` and cannot copy what it cannot find — `bundle: cannot
resolve libpython3.8.so.1.0` is precisely how this host stayed broken. It then
recognises the library as already ours and walks its needs without copying it
onto itself, the case added when the bundler was first shared.

**arm64 now has MORE working Python than x86_64.** `struct` and `math` are built
into Ubuntu's libpython (`nm -D` finds `PyInit__struct`) and 41 more modules
arrive in lib-dynload, all resolving against the shared library gdb loaded. On
x86_64 no extension can ever load, because ARM's static build exports no C-API.

Proven on the arm64 runner, which is the only place it can be:

    gdb-python: gdb LINKS libpython3.8 — taking it from Ubuntu focal
    bundled 5 libs into lib/ (rpath $ORIGIN/../lib)
    gdb-check: runs — GNU gdb (Arm GNU Toolchain 13.2.rel1 ...)
    gdb-check: embedded python — PYOK 3.8.10
    gdb-check: extension modules load (shared libpython)

The verification moved OUT of `ship_gdb_python` into `verify_gdb_runs` so it
runs after bundling: on arm64 gdb also needs the ncurses the bundler supplies,
so checking earlier would have failed for the wrong reason.

The arm64 tarball's sha256 finally CHANGED — it had been byte-identical to
-nros2 across three releases, which is how each of those proved it had touched
only x86_64.

## Residue — SURVEYED AND HALVED, 2026-09-21 (`-nros5`)

**This section is the live description of what the artifact does.** The
phase-447 doc's "needs a re-cut in nano-ros-sdk" line is discharged by it; see
the dated note there.

The original text of this section named `_ssl` and `_hashlib` and said the
whole set was "a few modules". It was written from reading, not measured, and
it was wrong by half. Sweeping all 44 lib-dynload modules (their `DT_NEEDED`
from the focal arm64 debs the build already extracts, against jammy's package
set) found **four** unimportable, and the one it missed is the one that
matters:

| module | needs | on jammy | now |
| --- | --- | --- | --- |
| `_ctypes` | `libffi.so.7` | not installed by default | **bundled** |
| `_decimal` | `libmpdec.so.2` | **no such package** (jammy has libmpdec3) | **bundled** |
| `_hashlib` | `libcrypto.so.1.1` | **no such package** (jammy has libssl3) | still fails |
| `_ssl` | `libssl.so.1.1` + `libcrypto.so.1.1` | **no such package** | still fails |

The other nine externals (bz2, lzma, sqlite3, ncursesw, tinfo, panelw,
readline, db-5.3, uuid) jammy does ship. `ctypes` has no pure-Python fallback
and is what a gdb pretty-printer can plausibly reach; `decimal` falls back to
`_pydecimal`, but its library was in the same walk and costs 215 KB.

**The TLS pair is still declined, and that half of the original text stands**:
a deprecated OpenSSL shipped so `import ssl` works *inside a debugger* is a
cost with no user, and `nros-sdk-index.toml` already declares
`[prereq.libssl3]` for other tools — two OpenSSL majors in one store is worse
than an unimportable `ssl`. The failure stays loud: the ImportError names the
library.

### Why not the policy fix

The structural cause is that `bundle_linux_libs` is called with
`"$topdir"/bin/*`, so the lib-dynload `.so`s sit in no bundler root and the
policy that makes the rest of the dist self-contained never sees them. Passing
them as extra roots was the preferred fix and is blocked three ways, measured:

1. the bundler resolves through `ldd` and `exit 1`s on a soname it cannot
   resolve — and `libmpdec2`/`libssl1.1` are *no such package* on jammy, so
   they cannot be installed for it to find. (`libncurses5` works as a
   precedent only because jammy still ships it.) Passing lib-dynload as roots
   therefore fails the build unless OpenSSL 1.1 is bundled, the one thing ruled
   out.
2. it applies one hardcoded `$ORIGIN/../lib` rpath to every root — correct for
   `bin/`, wrong four directories down.
3. its verification pass shares that assumption.

Changing a helper five dists share to buy two libraries is the wrong trade, so
`-nros5` stages the two from the focal pool (the same route libpython already
takes) and applies the rpath **uniformly to every lib-dynload module**, so no
module is named and a future ARM bump that adds one is covered.

### And the residue is now measured, not described

`verify_libdynload_residue` imports every lib-dynload module inside the shipped
gdb and fails the build when the unimportable set differs from its recorded
baseline **in either direction** — a module that starts failing is a
regression, one that starts working is a fix, and both want a human. A probe
that produces no verdict is a failure, not an empty residue. That is what stops
this section drifting again. The arm64 runner printed:

```
gdb-check: lib-dynload 44 module(s), unimportable = [_hashlib _ssl] (baseline)
```

x86_64 is untouched throughout: its gdb links a static CPython exporting no
C-API symbols, so no extension module can load there on any host, and the probe
is guarded on a shared libpython.
