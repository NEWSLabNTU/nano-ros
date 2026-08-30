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

## Directions

1. **Set `PYTHONHOME` from a launcher**, the way the macOS bundler already wraps
   binaries — if ARM's gdb can be pointed at a stdlib we ship. Needs checking
   whether 13.2's gdb accepts a 3.10 stdlib or hard-wants 3.8.
2. **Ship the matching Python stdlib in the dist.** Larger, but it is what makes
   the dist genuinely self-contained rather than self-contained-except-gdb.
3. **Take ARM's non-Python gdb if one exists** for the release we pin, or a
   newer ARM toolchain whose gdb links the Python the runner has.
4. **Accept and say so** — document that `arm-none-eabi-gdb` needs a host Python
   and leave the rest of the toolchain bundled. Cheapest, and honest, but it
   leaves `nros setup --tool arm-none-eabi-gcc --check` reporting `[OK]` for a
   toolchain with a broken debugger, which is precisely the shape 0926 removed
   for openocd.

Direction 4's caveat is the reason this is filed rather than dropped: a green
check over a broken binary is the failure mode this whole campaign existed to
remove, and it has quietly reappeared in one place.
