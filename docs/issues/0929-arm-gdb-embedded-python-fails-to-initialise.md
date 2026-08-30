---
id: 929
title: "`arm-none-eabi-gdb` starts but produces no output — ARM's embedded
  Python cannot initialise, and no declaration can fix it"
status: open
type: bug
area: tooling
related: [0926, 0928]
---

## Symptom

With `arm-none-eabi-gcc` 13.2-nros2 the debugger gets PAST the loader — 0928
bundled the ncurses 5 it used to die on — and still does nothing useful:

    $ arm-none-eabi-gdb --version           # stdout: empty, exit 0
    Could not find platform independent libraries <prefix>
    Could not find platform dependent libraries <exec_prefix>
    Consider setting $PYTHONHOME to <prefix>[:<exec_prefix>]

Everything is on stderr; gdb aborts during Python init before printing its own
version. `--batch -ex 'print 2+2'` likewise prints nothing.

The compiler, assembler, linker and the other 29 binaries are unaffected.

## Why 0928 could not fix it, and why declaring it would be worse

The two hosts fail differently, and neither is reachable from the index:

* **linux-x86_64** embeds Python STATICALLY — there is no `libpython` in gdb's
  `NEEDED` list at all. So `check-dist-runtime-deps` cannot see it (it measures
  sonames), and no `system = [..]` entry can name it. What gdb wants is a
  Python *stdlib on disk* at a path it derives internally.
* **linux-arm64** links `libpython3.8.so.1.0` dynamically. Ubuntu 22.04 ships
  Python 3.10 and has **no python3.8 package at all**, so a prereq naming it
  would tell the user to install something apt cannot provide — worse than
  silence. This is why 0928 scoped bundling to x86_64 and made the arm64 leg
  say why it skips.

So the gate is not blind by oversight: a statically-embedded interpreter is
outside what "which shared libraries does this need" can express. Worth stating,
because the natural next move — "add a prereq" — is the wrong one.

## Note on measurement

First observed on a host whose `PYTHONPATH` pointed at ROS's 3.10
site-packages, which made the message look like ROS contamination. It is not:
the same failure occurs under `env -u PYTHONPATH -u PYTHONHOME`. Check that
before blaming the environment — the ROS path in the error text is a red
herring, the same shape as 0774 and as the `LD_LIBRARY_PATH` trap the 0928
bundler now guards against.

## What it actually is, and what actually blocks it (measured 2026-08-30)

**"ARM's embedded Python" is CPython 3.8 statically linked into gdb**, built
inside ARM's own container. The binary has no `libpython` in `NEEDED`; the
interpreter is compiled in, and its `sys.prefix` is baked to a path that exists
on no user machine:

    program name = '/usr/local/bld-tools/bld-tools-virtual-env/bin/python'
    searched:      /usr/lib/python38.zip, /usr/lib/python3.8
    Fatal Python error: init_fs_encoding: failed to get the Python codec
      of the filesystem encoding

Note **Fatal**. This is not a warning gdb continues past — the interpreter
aborts during init and takes gdb with it, which is why `--version` prints
nothing while exiting 0. The static interpreter still needs a Python 3.8
**stdlib on disk** to find `encodings`.

**Nothing blocks the x86_64 fix. It is proven and it is small.** Pointing
`PYTHONHOME` at an unmodified CPython 3.8 `Lib/` — pure Python, no compiled
extension modules, no build — makes gdb fully work:

    $ PYTHONHOME=<dir> arm-none-eabi-gdb --batch -ex 'python print(6*7)' -ex 'print 2+2'
    42
    $1 = 4
    $ ... -ex 'set architecture arm' -ex 'show architecture'
    The target architecture is set to "arm".

Cost, measured: the full 3.8 `Lib/` is 46 MB; dropping `test`, `idlelib`,
`tkinter`, `lib2to3`, `distutils`, `ensurepip`, `turtledemo`, `pydoc_data` and
`unittest/test` leaves **11 MB on disk / 1.6 MB compressed**, with gdb still
fully functional. That is the whole price of direction 2 on x86_64.

**linux-arm64 is a different and harder problem.** There the interpreter is NOT
static — gdb links `libpython3.8.so.1.0` — so a stdlib alone is insufficient and
the shared library is not in Ubuntu 22.04's archive either. It needs the `.so`
from somewhere (a python.org build, deadsnakes, or building 3.8) before the same
launcher trick applies. Splitting the hosts is legitimate: x86_64 is the primary
host and is one small change away.

## Directions

1. ~~**Set `PYTHONHOME` from a launcher**~~ + 2. ~~**ship the stdlib**~~ —
   **these are one direction and it is MEASURED (above): a trimmed 3.8 `Lib/`
   at 1.6 MB compressed, plus a launcher exporting `PYTHONHOME`, and gdb works.**
   The launcher idiom already exists in the sdk repo (`bundle_macos_libs` wraps
   binaries to set `DYLD_LIBRARY_PATH`); this is the same move for one variable
   on Linux. The open question is not feasibility but packaging: whether the
   stdlib ships inside the arm-none-eabi-gcc dist or as its own indexed tool
   that several dists could share.
3. **Take ARM's non-Python gdb if one exists** for the release we pin, or a
   newer ARM toolchain whose gdb links the Python the runner has.
4. **Accept and say so** — document that `arm-none-eabi-gdb` needs a host Python
   and leave the rest of the toolchain bundled. Cheapest, and honest, but it
   leaves `nros setup --tool arm-none-eabi-gcc --check` reporting `[OK]` for a
   toolchain with a broken debugger, which is precisely the shape 0926 removed
   for openocd.

Direction 4's caveat WAS the reason this is filed rather than dropped: a green
check over a broken binary is the failure mode this whole campaign existed to
remove, and it had quietly reappeared in one place.

## That caveat is fixed (2026-08-30) — the defect itself is not

`[tool.*]` gained `smoke = [{ run, expect }]`: commands that must WORK once the
dist is installed. `system = [..]` asks whether the libraries a dist links are
present; this asks whether the thing does anything, and gdb is the proof the two
differ — it resolves every library it names, exits 0, and prints nothing.

`expect` matches a substring of the OUTPUT rather than an exit status, because a
status probe passes this exact bug. Probes run with `LD_LIBRARY_PATH`,
`PYTHONPATH` and `PYTHONHOME` removed so they measure the dist and not the
caller's shell — the contamination that sent this issue's first diagnosis down a
blind alley.

All three surfaces that could have lied now report it:

    nros setup --tool arm-none-eabi-gcc --check
      [BROKEN]  tool  arm-none-eabi-gcc 13.2-nros2 — installed, every declared
                library present, and `bin/arm-none-eabi-gdb --version` does not
                work: expected `GNU gdb`, printed NOTHING
    nros setup --check   -> same line, and "1 installed but not working"
    just doctor          -> same line, and doctor goes red

Declared for healthy dists too (openocd, qemu, riscv-none-elf-gcc), which all
report `[OK]`: a check that exists only where something is known broken tests
one binary, while declared across the class it tells you when a working one
stops working.

So direction 4 is now a legitimate choice rather than a quiet lie. The debugger
is still broken, and this issue stays open for that.
