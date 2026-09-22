---
id: 1452
title: "`check-dist-runtime-deps` walks a bundled CPython's optional extension
  modules, so on an arm64 host `just doctor` reports 12 undeclared sonames —
  and declaring two of them would be actively harmful"
status: open
type: tech-debt
area: tooling, build
severity: low
found: 2026-09-22
related: [issue-0926, issue-0932, issue-0928, issue-0929, issue-0196, rfc-0099, phase-447]
---

## What was measured

`scripts/check-dist-runtime-deps.py` derives a provisioned dist's external
shared-library closure and requires every soname in it to be declared. The
derivation is sound and is the whole point of the gate (issue 0926): it walks
every ELF under `<store>/<tool>/<pinned version>`, runs `ldd` with
`LD_LIBRARY_PATH` stripped, subtracts the base glibc/gcc runtime, the
rustc-shaped sonames and everything the dist ships itself, and requires the
remainder to be covered by `[tool.<name>] system = [..]` through each
`[prereq.*]`'s `check.sharedlib` plus its `provides = [..]`.

On an **arm64 host** that has installed `[tool.arm-none-eabi-gcc]`, that closure
is **13 sonames** and `system = ["libcrypt1"]` covers exactly one of them. The
other 12 come from ONE place: the CPython that `arm-none-eabi-gdb` links on that
host, and specifically its `lib-dynload/*.so` optional extension modules, each
of which names a library of its own.

| what names it | library |
| --- | --- |
| `_bz2` | `libbz2` |
| `_lzma` | `liblzma` |
| `_sqlite3` | `libsqlite3` |
| `_curses` / `_curses_panel` | `libncursesw`, `libtinfo`, `libpanelw` |
| `readline` | `libreadline` |
| `_dbm` | `libdb-5.3` |
| `_uuid` | `libuuid` |
| `nis` | `libnsl` |
| `_ssl`, `_hashlib` | `libssl.so.1.1`, `libcrypto.so.1.1` |

The consequence is concrete rather than theoretical: the gate is wired into
`just doctor` (`just/workspace.just`), where a non-zero result sets `fail=1`. So
on such a host the recipe everyone is told to trust for tier preconditions goes
red with a finding that is not a defect, and it prints the remedy the gate was
written to print — "add a `[prereq.*]` entry" — for twelve libraries the
toolchain does not need in order to work.

## It predates the -nros5 re-cut

This is not fallout from recent work. `arm-none-eabi-gcc` **13.2-nros4** shipped
the same `lib-dynload` set and **two MORE** undeclared sonames than -nros5 does
— `libffi.so.7` (`_ctypes`) and `libmpdec.so.2` (`_decimal`), which -nros5
bundles into the dist and which therefore now subtract out as "shipped by the
dist". So the re-cut made this report two sonames SHORTER; it did not create it.

The arm64 leg has linked a dynamic CPython since -nros4, which is archived issue
0932's fix: on that host gdb links `libpython3.8.so.1.0` and fails at the loader
before any interpreter runs, so -nros3's stdlib-only fix (issue 0929) could not
help and the dist took Ubuntu focal's arm64 library instead. A dynamic libpython
brings a real `lib-dynload`, and a real `lib-dynload` brings one optional
library per module. The x86_64 leg cannot reach this state at all: its gdb
embeds CPython statically and exports zero Python C-API symbols, so no `.so`
extension module loads into it on any host.

The `[tool.arm-none-eabi-gcc]` entry in `nros-sdk-index.toml` records the
consequence in a comment beside the `system` declaration, which is where this
issue was harvested from. That comment is the statement of the problem; this
file is the tracked question.

## Why this is a gate-reach question, not a dist defect

`system = [..]` declares **what the dist needs in order to FUNCTION**. A
cross-compiler toolchain and its debugger need none of the twelve: they are the
libraries behind Python extension modules that a pretty-printer may or may not
reach, and every one of them has the same disposition as a missing optional
module — that `import` fails and nothing else does.

The gate's closure, by contrast, is derived from **ELF presence**: a file with
`.so` in its name inside the dist tree gets `ldd`'d and its needs counted. For
every dist in the store but this one those two definitions agree, because the
only shared objects present are ones the tool loads to run. A bundled
interpreter's plug-in directory is the first case where they diverge.

So the open questions are about the gate's reach, and there are two:

1. **Which files should the closure walk?** Every ELF, or only those reachable
   from what the dist's launchers actually exec?
2. **Should an optional extension module be in scope at all?** "This dist
   contains a shared object that needs X" and "this dist needs X" are different
   propositions, and the gate currently only expresses the first.

Both are issue 0196's shape, and both directions of it. Today the gate's reach
is WIDER than the rule it enforces, which produces a false report. Any narrowing
risks making it NARROWER than the rule, which is the failure mode that let
`[tool.qemu] system = ["libslirp"]` stand while 19 sonames went undeclared —
the defect this gate exists for.

## Declaring the sonames is not the cheap way out — for two of them it is harmful

The obvious "just declare them" reflex is wrong here, and specifically wrong for
`libssl.so.1.1` / `libcrypto.so.1.1`.

Those two sonames are **unobtainable on jammy**: OpenSSL 1.1 is not packaged on
22.04 by any of the managers the index knows. phase-447 D1's **backward half**
(RFC-0099, `docs/design/0099-provisioning-is-planned-once-and-prefers-prebuilts.md`)
reads `system = [..]` against D9's per-release package names and **refuses a
prebuilt** when a soname is ABSENT and the index says the manager does not
package it on this release. Declaring the pair would therefore make the
provisioner refuse the ENTIRE arm-none-eabi toolchain — gcc, gdb, everything —
on every jammy arm64 host, over two Python modules that cannot be imported. A
working cross compiler would become uninstallable in exchange for silencing a
report.

The remaining ten are less dramatic and still wrong in kind: each would add a
`[prereq.*]` entry telling a user to `apt install` a package so that a debugger
they are not using can import a module they are not importing, and every one of
them is a new row that `nros setup --system --check` then reports as `[MISSING]`.

## Why nobody has seen it

* **No CI lane can reach it.** Every runner in `.github/workflows/` is x86_64,
  and the x86_64 leg cannot produce the condition (static CPython, no extension
  modules). So there is no lane whose verdict would change.
* **The gate needs a provisioned store**, so it is deliberately not in any
  affordability tier (`check-lane-contracts`) and runs only from `just doctor`
  and `just check dist-runtime-deps`.
* Intersect those two and the audience is exactly one: a developer on an arm64
  Linux host who has run `nros setup --tool arm-none-eabi-gcc`. That is a
  supported host — the index publishes a `dist.linux-arm64` for it — but nobody
  in this project is currently working from one, which is why the finding
  reached a comment rather than a red lane.

This is worth stating plainly because it also bounds the severity: the defect is
a false report on a host nobody here uses today, not a broken build. It is filed
so that the first person to work from an arm64 host meets a tracked question
instead of a mystery, and so the index comment is not the only record.

## Fix candidates — stated, none chosen

Deliberately unpicked: choosing between these is a decision about what `system`
MEANS, and that belongs to whoever next touches RFC-0099's D1/D9 half rather
than to the person who noticed the report.

1. **Declare all 12.** Rejected above for the TLS pair on D1 grounds; it is
   recorded here as the candidate that looks cheapest and is not available.

2. **Exclude a bundled interpreter's plug-in directory from the walk.** Smallest
   diff, and the one with the sharpest trap: keyed on a path substring
   (`lib-dynload`) it is a name-match that any second bundled interpreter with a
   different layout would slip past, and it would hide a genuinely undeclared
   dependency of a module that IS required. If this is taken it wants a negative
   control that proves a real omission inside an excluded directory still fails.

3. **A second index field for optional sonames** — a `system_optional = [..]`
   (spelling not proposed) that the gate accepts as coverage and D1's backward
   half deliberately does NOT read, so absence never refuses a prebuilt. This
   makes the distinction explicit and truthful in the index, at the cost of a
   second meaning beside `system` and a third reader of both. Note that
   `provides = [..]` already exists for a different purpose (one prereq covering
   several sonames) and must not be overloaded into this one.

4. **Derive the closure by REACHABILITY rather than by presence** — start from
   the entry points the dist's launchers exec and follow `NEEDED` transitively,
   so a plug-in nothing loads at startup is out of scope by construction. This
   is the principled answer and the most work, and it has its own hole: a
   `dlopen`ed plug-in that IS required (openocd's `libftdi` road, issue 0926) is
   invisible to a `NEEDED` walk, so reachability alone would weaken the gate
   exactly where it first earned its keep.

5. **Accept and declare the report.** Leave the derivation alone and teach the
   gate to print these as a named, reasoned exemption for this dist rather than
   as findings — the shape `RUSTC` already has in the same script for
   rustup-shipped libraries. Costs the least, and it puts a per-dist exemption
   list into a gate that currently keeps no table of its own, which was an
   explicit design property of it (the soname mapping lives in the index
   precisely so a second table cannot drift from it).

## What acceptance has to look like

Not a green gate on x86_64 — every candidate above is green here today, because
here the closure is clean. Whatever is taken has to be measured on an **arm64
host with the toolchain provisioned**, and it has to show both directions:

* the twelve `lib-dynload` sonames no longer reported, AND
* a deliberately un-declared REAL dependency of the same dist still reported.

Without the second half this is issue 0196 again, one layer down: a gate
narrowed until the noise stopped, with no evidence it can still fail.
