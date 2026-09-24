---
id: 1452
title: "`check-dist-runtime-deps` walks a bundled CPython's optional extension
  modules, so on an arm64 host `just doctor` reports 12 undeclared sonames —
  and declaring two of them would be actively harmful"
status: resolved
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

---

# Resolved — candidate 4, with the roots derived from the artifact

Taken: **candidate 4, reachability**, in a form the "fix candidates" section did
not quite anticipate, and NOT candidate 2 (path exclusion), 3 (a second index
field) or 5 (a per-dist exemption table).

**The rule now.** Scope = the dist's programs, plus every library a program's
`DT_NEEDED` chain reaches inside the dist. A program is an ELF the loader can exec —
`ET_EXEC`, or `ET_DYN` carrying a `PT_INTERP` — read from the ELF header. A
dist with libraries and no program at all is a LIBRARY dist and all its
libraries are roots, because otherwise such a dist would measure the empty set
and print OK, which is a gate that can only pass.

**Two things about the shape of candidate 4 that the filing got wrong**, both
measured rather than argued:

1. *"a `dlopen`ed plug-in that IS required (openocd's `libftdi` road) is
   invisible to a `NEEDED` walk"* — `libftdi.so.1` is a **`DT_NEEDED` of the
   `bin/openocd` PROGRAM** (`readelf -d`), which is exactly why it failed at the
   loader (`error while loading shared libraries`) rather than at a `dlopen`.
   The catch this gate first earned its keep on survives the narrowing
   untouched, along with `libhidapi`, `libusb` and — transitively, through
   libusb — `libudev`.
2. Rooting the walk in **what the index declares** (`front`, `smoke`), which is
   the obvious reading of "what the tool offers", is *worse* than rooting it in
   the artifact. `smoke` for `arm-none-eabi-gcc` names 2 of ~40 shipped
   binaries; a user runs `arm-none-eabi-objcopy` too, and `[tool.qemu]` smokes
   `qemu-system-arm` while the tree also runs `qemu-system-riscv64`. Worse in
   kind: `system = [..]` is hand-authored and *"only ever as complete as whoever
   wrote it"* is the sentence this gate exists to answer — deriving its reach
   from a second hand-authored field re-creates that dependency one level up.

**Why `ldd` could not be left to do the transitivity.** Measured on
`[tool.xrce-agent]` as provisioned: `bin/MicroXRCEAgent` is a launcher SCRIPT,
the program is `lib/MicroXRCEAgent.real`, and its `ldd` stops at
`libmicroxrcedds_agent.so.2.4 => not found` (the launcher supplies the path at
exec time). So `libssl.so.3` / `libcrypto.so.3`, three links down and genuinely
required, are invisible to a program-rooted walk that trusts the loader. A first
draft of this fix did exactly that and **lost both of them on the author's own
host** — the reason the chain is walked from parsed `DT_NEEDED`, resolved
against the dist's own filenames *and* `DT_SONAME`s.

## What this stops catching, and why that is acceptable

**A dlopen'd plug-in's own dependencies.** If such a plug-in is REQUIRED rather
than optional, a library behind it can now go undeclared and surfaces at first
use as a `dlopen` failure instead of here.

This is not hypothetical and the issue's framing understated it: the store
already contains required dlopen plug-ins. `arm-none-eabi-gcc` ships
`libexec/.../liblto_plugin.so` and `lib/bfd-plugins/libdep.so`, which `ld`
dlopens during an LTO link; `riscv-none-elf-gcc` ships those **plus 75 CPython
extension modules under `lib/python3.12/lib-dynload/`** — the arm64 class is
already present on x86_64, one distro build difference away from producing the
same report here.

Bounding it, measured. The 88 out-of-scope objects across the provisioned store
break down as 75 riscv `lib-dynload` modules, 4 gcc/BFD plug-ins, 8 riscv
`libexec/` libraries that only those plug-ins reach, and one
`share/qemu/s390-ccw.img` (an s390 firmware blob, never a host object at all).

* **Not one of the 88 names anything outside the base glibc/gcc runtime** — and
  the reason is worth recording, because it is the alternative arm64 declined:
  `riscv-none-elf-gcc` **BUNDLES** what its modules need
  (`libexec/libssl.so.3`, `libcrypto.so.3`, `libsqlite3.so.0.8.6`,
  `libffi.so.8.1.4`, `libnsl`, `libpanel`, `libcrypt`), so they subtract out as
  shipped. -nros5 bundled `libffi`/`libmpdec` for the same reason and
  deliberately did NOT bundle the deprecated OpenSSL 1.1 pair.
* It is not made invisible. An out-of-scope object that names an external
  library is **reported as a note** — never a verdict — so the arm64 twelve
  would still be printed, green, saying what they are.
* `--include-unreached` restores the old measurement in one flag.

## Acceptance

The arm64 symptom could **not** be reproduced: the symptom needs an arm64 host
with `arm-none-eabi-gcc` 13.2-nros5 installed, and the author's host is x86_64
with -nros1/-nros4. What was done instead:

* **The structure was reproduced on x86_64.** `riscv-none-elf-gcc` 14.2-nros1
  bundles a dynamic `libexec/libpython3.12.so.1.0` *and* its
  `lib/python3.12/lib-dynload/`. Under the new rule the libpython is IN SCOPE
  (a program reaches it) and its 75 plug-ins are not — the arm64 shape exactly,
  on real bytes.
* **No regression, measured per dist.** Scoped closure vs the pre-fix
  every-ELF closure over the 8 pinned-and-present dists: **difference 0**.
  `[tool.qemu]` keeps `libselinux.so.1` + `libpcre2-8.so.0`, `[tool.xrce-agent]`
  keeps `libssl.so.3` + `libcrypto.so.3`, `[tool.openocd]` (0.12.0-nros1, not
  the pin) keeps all four including `libudev.so.1`.
* **A recorded shape, both directions.** `ARM64_GDB_SHAPE` in the gate is the
  arm64 dist transcribed from the table above; `XRCE_SHAPE` is the chain `ldd`
  cannot follow. Eight reachability rows assert on them, and the two-directional
  half is the mutations: a plug-in the program NEEDS is back in scope, a
  program's own undeclared dependency is still measured, an internal chain is
  still walked, a library-only dist still measures its libraries.
* **The self-test rows were themselves mutation-tested.** Each of these source
  mutations is caught, each by a different row — re-runnable by applying them by
  hand and running `python3 scripts/check-dist-runtime-deps.py`:

  | mutation | caught by |
  | --- | --- |
  | `dist_scope` returns everything (the pre-fix behaviour) | every lib-dynload plug-in is out of scope |
  | drop the transitive walk (roots only) | 5 rows, incl. the internal chain |
  | exclude on the `lib-dynload` path substring (candidate 2) | a plug-in a program NEEDS is back in scope |
  | a library-only dist measures nothing | a dist with no program measures its libraries |
  | `elf_facts` calls every ELF a program | a real shared library is NOT classified as a program |
  | `elf_facts` calls every ELF a library | this interpreter is classified as a program |
  | `coverage_problem` never complains | 2 rows |
  | `coverage_problem` always complains | 2 rows |
  | `sonames_of` ignores `provides` | 2 rows |

* **The ELF classifier is probed against real bytes on every run**, not only
  against the table: this interpreter must classify as a program and one of its
  own resolved dependencies must classify as a library with a `DT_SONAME`. A
  table alone would let the classifier answer "program" to everything.
* **End to end, on a synthetic store built from real host ELFs** — because the
  author's own store reaches neither the note path nor the failure path. One
  `[tool.openocd]`-shaped tree under `--store`, three runs, **the same soname
  each time**:

  | the tree | result |
  | --- | --- |
  | `lib/plugin.so` (a copy of the host `libssl.so.3`) that no program reaches | `rc=0`, a NOTE naming `libcrypto.so.3`, **verdict on the first line** |
  | the same tree, `--include-unreached` | `rc=1`, `libcrypto.so.3 — declared by [prereq.libssl3], not in system = [..]` |
  | the same need on the PROGRAM (a copy of the real `bin/openocd`) | `rc=1`, four findings: `libftdi.so.1`, `libhidapi-hidraw.so.0`, `libusb-1.0.so.0` and — transitively through libusb — `libudev.so.1` |
  | the same library ALONE, no program anywhere in the tree | `rc=1`, `libcrypto.so.3` — the library-only-dist clause, which has no example in the real store |

  Rows 1 and 2 differ only in the flag; rows 1 and 3 only in *which file* names
  the library; rows 1 and 4 only in whether a program is present at all. That
  is the narrowing doing exactly what it claims and nothing more. Row 1's first
  line matters on its own: `just doctor` renders this gate by `head -1`, so
  notes print last, after the verdict, on both paths.

## Left alone, deliberately

* `nros-sdk-index.toml` is **unchanged apart from its comment**. That the fix
  needed no declaration to move is the evidence that `system = ["libcrypt1"]`
  was right.
* `scripts/sdk/measure-dist-floor.py` still reads **every** ELF, and should.
  "Can these bytes run on this host at all" is a property of the file; a
  `lib-dynload` module with a higher `GLIBC_x.y` reference genuinely does raise
  the artifact's floor even though nothing execs it. Same walk, different
  question.
* The gate keeps **no table of its own** — the property candidate 5 would have
  cost. The soname mapping still lives only in the index.

Side effect worth naming: `file` and the per-file `ldd` are gone from the walk
in favour of reading the ELF header directly, so the gate runs in **0.4 s where
it took 27 s**. It gains no dependency — the sibling `measure-dist-floor.py`
keeps using `readelf` because it needs `.gnu.version_r`, which is a real reason
to shell out.
