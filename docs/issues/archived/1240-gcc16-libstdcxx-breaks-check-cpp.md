---
id: 1240
title: "`just check cpp` cannot run on a GCC 16 host — `__has_include` says `<string>` is available under `-ffreestanding`, and GCC 16's libstdc++ `#error`s on it"
status: resolved
type: limitation
area: cpp, build, ci
severity: medium
found: 2026-09-09
related: [0112, 0872, 1187, 1223]
resolved_in: "(this commit)"
---

# The compile tier is unrunnable on this host, and the instruction assumes it is not

CLAUDE.md is unambiguous: **green CI locally BEFORE pushing** — run `just format`
and the tier your change earns, fix every failure, so the push passes remote CI
on the first try. On a host with GCC 16 that instruction cannot be followed for
anything that reaches `check-cpp`: the lane fails before it has compiled a line
of nano-ros code.

Measured 2026-09-09 on Arch, `gcc (GCC) 16.1.1 20260728`, running `just ci gate`
on a branch whose entire diff is one shell script.

## What fails

Two distinct failures inside `/usr/include/c++/16`, both raised while compiling
our headers but neither in our code:

```
/usr/include/c++/16/bits/stl_vector.h:2195:11: error: there are no arguments to
  '__throw_length_error' that depend on a template parameter, so a declaration
  of '__throw_length_error' must be available [-Wtemplate-body]

/usr/include/c++/16/sstream:1150:57: error: no type named 'pos_type' in
  'std::__cxx11::basic_stringstream<wchar_t>::traits_type'
  {aka 'struct std::char_traits<wchar_t>'}
```

`-Wtemplate-body` is new in GCC 16: it diagnoses two-phase-lookup problems in
template bodies that earlier releases only reported at instantiation. The lane
compiles with `-Werror`, so a warning in a SYSTEM header becomes an error — and
these are libstdc++'s own template bodies, not ours.

## The cascade, which is what makes the log misleading

Once `<vector>` and `<sstream>` have failed, `<memory>` is not usable either, so
`NROS_CPP_HAS_SHARED_PTR` never gets defined and the compiler reports:

```
packages/api/nros-cpp/include/nros/timer.hpp:68:28: error: 'shared_ptr' in
  namespace 'std' does not name a template type
  note: 'std::shared_ptr' is defined in header '<memory>'; this is probably
  fixable by adding '#include <memory>'
```

That note is wrong here, and it is the trap. `timer.hpp` DOES include `<memory>`
— twice, once under `NROS_CPP_STD` and once behind the `__has_include` probe
phase-417 W1.a added for issue 0112. Taking GCC's suggestion would add a third
copy of an include that is already there and fix nothing. **The `shared_ptr`
errors are downstream of the libstdc++ failure above them; read the FIRST error
in this lane, never the most specific-looking one.**

## Why nothing caught it

CI runs `ubuntu-22.04` (GCC 11), where neither diagnostic exists, so the lane is
green there and will stay green. `check-cpp` is in the compile tier, which the
`pre-push` hook deliberately excludes, so a contributor on a rolling distro
meets this only when they follow the instruction to run the tier locally — which
is the population most likely to be doing the right thing.

Same shape as issue 0872 one axis over: an arm that has never run to completion
in one environment, where each fix exposes the next gap. Distinct from issue
1187, which is `<string>` reaching an embedded FreeRTOS C++ image — that is our
own porting surface, this is a host toolchain we do not build against anywhere.

## Reproduce

```bash
just check cpp     # on a GCC >= 16 host
```

First error is in `/usr/include/c++/16/...`, not under `packages/`.

# RESOLUTION (2026-09-09): the diagnosis above was wrong, and all three proposed fixes were the wrong lever

`just check cpp` is GREEN on this host: `gcc (GCC) 16.1.1 20260728`, Arch,
exit 0, **zero** `error:` lines. Both reported failures are cleared, and they
turned out to have ONE cause — which is neither `-Werror` nor `-Wtemplate-body`.

## What is actually wrong

**There is no `-Werror` on the failing command line.** The freestanding header
probe in `just/check/lanes.just` is

```
c++ -fsyntax-only -std=c++14 -ffreestanding -fno-exceptions -fno-rtti \
    -I... -include "$hdr" -x c++ /dev/null
```

The lane's only `-Werror` spellings are targeted (`-Werror=deprecated-declarations`,
`-Werror=unused-result`) and are on other TUs. So the premise this issue was
filed on — "the lane compiles with `-Werror`, so warnings in system headers
become errors" — does not hold, and every fix derived from it is aimed at
nothing.

The real trigger is `-ffreestanding` meeting GCC 16's libstdc++, which is
genuinely freestanding for the first time. Reduced to two lines:

```
$ printf '#include <string>\nint main(){return 0;}\n' > probe.cpp
$ c++ -fsyntax-only -std=c++14 -fno-exceptions -fno-rtti probe.cpp          # clean
$ c++ -fsyntax-only -std=c++14 -ffreestanding -fno-exceptions -fno-rtti probe.cpp
/usr/include/c++/16/bits/requires_hosted.h:34:4: error: #error "This header is
  not available in freestanding mode."
```

Everything else follows from that one `#error`. `bits/functexcept.h` never
declares `std::__throw_length_error` / `__throw_bad_alloc` / `__throw_logic_error`,
so libstdc++'s own template bodies reference undeclared names — which GCC 16
reports at DEFINITION with the new `-Wtemplate-body` tag rather than deferring to
instantiation. That tag is what made the log look like a warning policy problem.
It is not: these are errors, and they are errors about a header that refused to
be included.

**The `pos_type` error is the same cause, not a second problem.** In freestanding
mode libstdc++'s `char_traits<char>` and `char_traits<wchar_t>` omit
`pos_type`/`off_type` (they need `mbstate_t` from hosted `<cwchar>`), so every
`typedef typename _Traits::pos_type pos_type;` in `<streambuf>`, `<istream>`,
`<ostream>`, `<sstream>` fails. One fix clears both.

And the `shared_ptr does not name a template type` reports were, as the issue
already said, pure cascade.

## Why the three proposed fixes were measured and rejected

Each was run against the two-line repro above, counting `error:` lines:

| candidate | errors remaining |
| --- | --- |
| baseline | 15 |
| **(1)** `-isystem /usr/include/c++/16 …` | **15** |
| **(3)** `-Wno-template-body` | **9** |

(1) does nothing, for two independent reasons: those directories are ALREADY
system include dirs, and system-header suppression applies to warnings — an
`#error` is not one. (3) silences only the diagnostics carrying that tag; the
`#error`, `'strtol' is not a member of 'std'`, and every `pos_type` failure are
untagged and survive. (2) — pinning a host compiler — would have hidden a real
defect in our headers rather than fixing it.

## The actual defect, and it is ours

`__has_include(<string>)` answers **TRUE** under `-ffreestanding`. The FILE
exists; it just refuses to be included. Measured:

```
                          -ffreestanding    hosted
__STDC_HOSTED__                 0             1
__has_include(<string>)       TRUE          TRUE
```

Fourteen capability blocks across eleven `nros-cpp` public headers gated a
hosted STL include on `__has_include` ALONE:

```c
#if defined(NROS_CPP_STD)
#include <string>
#define NROS_CPP_HAS_STD_STRING 1
#elif defined(__has_include)          // <-- live arm in every shipped config
#if __has_include(<string>)
#include <string>
#define NROS_CPP_HAS_STD_STRING 1
#endif
#endif
```

That arm is live in every shipped configuration, because **nothing that ships
defines `NROS_CPP_STD`** — no cmake module, no toolchain file, no build.rs, no
Kconfig. So under `-ffreestanding` our headers asked libstdc++ for `<string>`,
`<sstream>`, `<memory>`, `<vector>` and `<functional>`, and got the `#error`.

## The fix — the conjunction, applied to the whole class

`nros.hpp` had already MEASURED the correct predicate two days earlier, for
`<chrono>` and for exactly this reason ("GCC 13 gates `bits/requires_hosted.h`
on `__STDC_HOSTED__` and `-ffreestanding` clears it"). It landed at that one
site only. This change applies it to all fourteen:

```c
#if defined(NROS_CPP_STD) || (defined(__STDC_HOSTED__) && __STDC_HOSTED__ \
    && __has_include(<string>))
#include <string>
#define NROS_CPP_HAS_STD_STRING 1
#endif
```

Neither probe alone is sufficient and both are necessary:

* `__STDC_HOSTED__` alone is wrong for issue 0112's reason — a hosted compiler
  run `-nostdinc++` against Zephyr's minimal libcpp reads HOSTED with no
  `<string>`.
* `__has_include` alone is wrong for THIS issue's reason — present but poisoned.

`NROS_CPP_STD` survives as the explicit consumer override. Deleting the
`__has_include` arm outright (which is what `.config/cpp-freestanding-includes-baseline.txt`
recorded as phase-438 W2's plan) would have removed `std::string`-taking
`get_logger`, the `SharedPtr` aliases and the `<vector>` options surface from
EVERY build including the hosted one — the same over-tightening `nros.hpp`
committed for `<chrono>` on 2026-09-05 and corrected on 2026-09-07.

Sweep command, for the next person:

```bash
grep -rn '__has_include' packages/api/nros-cpp/include/nros/*.hpp
```

## The gate, widened to the rule it enforces (0196)

`check-cpp-freestanding-includes` scored a region as guarded if the `#if` line
merely CONTAINED the token `NROS_CPP_STD`. Under that test the broken shape and
the corrected one are indistinguishable. Its walker now has `std_frame()`: a
directive naming `NROS_CPP_STD` whose live arm reaches `__has_include` WITHOUT
`__STDC_HOSTED__` is not a std region. Three selftest cases added (9 total), and
the new rule is mutation-proven — deleting the `__STDC_HOSTED__` condition makes
cases 8 and 9 fail, which is what a negative control is for.

`.config/cpp-freestanding-includes-baseline.txt` is now EMPTY: all fourteen
ratchet entries are paid. That was phase-438 W2's acceptance criterion, reached
by a different and safer route than the one W2 planned.

## What was measured

* `just check cpp` — **exit 0**, 0 `error:` lines, GCC 16.1.1. Was ~200 errors.
* Negative control, our own code: a `std::string` return type added to
  `publisher.hpp` outside a guard still fails the freestanding probe
  (`'string' in namespace 'std' does not name a type`). The probe has MORE teeth
  than before — previously `<string>` arrived via the ungated `__has_include`
  arm and the same code would have compiled clean.
* Negative control, warning policy: the lane's own expected-failure probes for
  `-Werror=unused-result` and `-Werror=deprecated-declarations` still report
  PASS, i.e. a discarded `[[nodiscard]]` and a deprecated spelling still fail
  the build. No warning flag was touched by this change.
* `check-cpp-freestanding-includes` — OK, 9 selftest cases, 0 baseline entries.
* `just check fast` — green apart from `check-xrce-vendored-versions`, which
  reports an unprovisioned `xrce-sys` submodule in this worktree and is
  unrelated.

## What is NOT closed by this

Issue **1187** (the FreeRTOS C++ carrier does not build) is the SAME defect on
arm-none-eabi 13.2 — `log.hpp`'s `<string>` was the entry point for 17 of the 19
failing headers, per the baseline file's own note — and this change should fix
it. That is not measured here: no arm-none-eabi FreeRTOS build was run. 1187
stays open until someone builds it.
