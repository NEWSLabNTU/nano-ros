// nros-cpp: standard-library capability detection — THE one site
// Freestanding C++ — this header includes nothing a minimal libcpp lacks
// unless it first establishes that the capability is there.

/**
 * @file std_detect.hpp
 * @brief The single definition of the `NROS_CPP_HAS_*` capability macros.
 *
 * Every other nros-cpp header includes this one and tests the macros. None of
 * them defines one. Before phase-438 W1 this block was hand-copied 15 times
 * across 11 headers, which is the "second spelling rather than one shared
 * helper" antipattern CLAUDE.md records for the Zephyr unset-variable guard
 * (#282 → #326), and it cost three separate things:
 *
 *   1. `nros.hpp` USED four of these macros and DEFINED none, so its behaviour
 *      depended on `publisher.hpp` / `options.hpp` / `subscription.hpp` having
 *      been included first. It was the only header in that state, and the
 *      failure mode is issue 0135's: two TUs of one image disagreeing about a
 *      capability, hence about a layout.
 *   2. The rationale drifted. Eleven of the copies carried a comment saying the
 *      test is `__has_include` "rather than `__STDC_HOSTED__`, issue 0112" —
 *      and issue 0112 says no such thing (see below).
 *   3. Fixing the gate meant fixing 15 sites. It now means fixing one.
 *
 * ## What decides a capability: the CONSUMER, not the toolchain
 *
 * `NROS_CPP_STD` is a request. A consumer defines it to say "compile me the
 * surface that lets an upstream rclcpp file build unmodified" — a PORTING
 * surface, which is a different question from "can this toolchain reach
 * `<memory>`". Two questions, and they used to share one name.
 *
 * Probing the include path for them was tried, and measured to be wrong on the
 * lane it mattered most (issue 1187): every embedded C++ FreeRTOS image failed
 * to compile, because
 *
 *                          __STDC_HOSTED__  __has_include(<string>)  #include
 *   arm-none-eabi 13.2                  0            TRUE          hard #error
 *     `-ffreestanding` (FreeRTOS lane)
 *   Zephyr `-nostdinc++`,               1            FALSE          absent
 *     minimal libcpp (issue 0112)
 *   hosted g++ 12.3                     1            TRUE          works
 *
 * Each probe is right on exactly one embedded lane and wrong on the other.
 * libstdc++ 13 added `bits/requires_hosted.h`, so on the FreeRTOS lane
 * `<string>` is PRESENT and including it is a hard `#error` — presence stopped
 * implying usability. Only the request is right on both, because it is not a
 * probe.
 *
 * ## Issue 0112 did not say what eleven comments said it said
 *
 * 0112's Resolution reads: "moved the `<string>` include into its own `#ifdef
 * NROS_CPP_STD` block, so it follows its actual consumer." It chose the opt-in
 * over `__STDC_HOSTED__`. It never chose `__has_include`, which arrived later
 * in `acf213871` and re-broadened the gate 0112 had narrowed. 0112's FINDING —
 * hostedness does not imply header availability — still holds; what changed is
 * that `__has_include` stopped being a fix for it.
 *
 * ## Widening, stated
 *
 * A consumer that needs only `<memory>` now also gets the other five when it
 * asks for the std surface. That is deliberate and it is what makes one site
 * possible. It is safe because `NROS_CPP_STD` means "this toolchain has the C++
 * standard library", not "this toolchain has some of it": the one in-tree
 * consumer that sets it says so in its own CMakeLists ("PX4 SITL is real posix
 * with full libstdc++, so opt in explicitly"). A toolchain with `<memory>` and
 * no `<sstream>` is not one that should be declaring `NROS_CPP_STD`.
 *
 * It also FIXES a latent version of issue 0135: with detection scattered, a TU
 * including only `client.hpp` and one including `nros.hpp` disagreed about
 * `NROS_CPP_HAS_STD_CHRONO`. Now every TU that includes any nros-cpp header
 * agrees about all six.
 */

#ifndef NROS_CPP_STD_DETECT_HPP
#define NROS_CPP_STD_DETECT_HPP

// --- The five uniform capabilities -------------------------------------------
//
// Uniform in the literal sense: before W1 these fourteen blocks normalised to
// one block text, with the header name and the macro name consistent at all
// three positions in every instance. `<chrono>` below is the one that does not,
// and it is kept separate rather than normalised into these.
//
// DISCOVERY IS GONE (phase-438 W2). The predicate is the request and nothing
// else. Issue 1240 had just corrected discovery at all fourteen sites to the
// two-probe conjunction
//
//     defined(NROS_CPP_STD) || (defined(__STDC_HOSTED__) && __STDC_HOSTED__
//                               && __has_include(<string>))
//
// because each probe alone is wrong on a lane the other gets right:
// `__STDC_HOSTED__` alone misreads Zephyr's `-fno-freestanding -nostdinc++`
// leaves as having `<memory>` (issue 0112), and `__has_include` alone answers
// "does the FILE exist", which is TRUE under `-ffreestanding` against a full
// libstdc++ whose `<string>` opens `#error "This header is not available in
// freestanding mode."` (issue 1240, made unmissable by GCC 16; issue 1187 is
// the same shape failing quietly on arm-none-eabi 13.2).
//
// That conjunction is a better PROBE. W2's claim is that no probe is the right
// instrument: the question a capability macro answers is which SURFACE the
// consumer asked to be compiled, not what the include path happens to hold.
// So the conjunction is removed rather than kept as a fallback, and the five
// consumers that need the porting surface make the request out loud -- the
// rclcpp compat shim, the phase-417 ported-surface probes, the four refusal
// probes, the `cpp_compat_snippets` compile-check arm, and this gate's own
// hosted arm.

#if defined(NROS_CPP_STD)
#include <memory>
#define NROS_CPP_HAS_SHARED_PTR 1
#endif

#if defined(NROS_CPP_STD)
#include <string>
#define NROS_CPP_HAS_STD_STRING 1
#endif

#if defined(NROS_CPP_STD)
#include <vector>
#define NROS_CPP_HAS_STD_VECTOR 1
#endif

#if defined(NROS_CPP_STD)
#include <functional>
#define NROS_CPP_HAS_STD_FUNCTION 1
#endif

// `<sstream>` is the `RCLCPP_*_STREAM` family only. It is worth noting that
// `log.hpp` pulls it, `qos.hpp` includes `log.hpp`, and every entity header
// includes `qos.hpp` — so a stream-formatting convenience sits on the
// transitive include path of every freestanding TU. That is a separate leak
// from how it is gated (phase-438, "which failures are real", cause (c)).
#if defined(NROS_CPP_STD)
#include <sstream>
#define NROS_CPP_HAS_STD_SSTREAM 1
#endif

// --- `<chrono>`: the block that got here first --------------------------------
//
// `<chrono>` reached the opt-in-only form one phase ahead of the other five,
// the hard way, and its comment is kept because it is the EVIDENCE for what
// phase-438 W2 then did to all of them. Three successive corrections, each
// measured against a build's own recorded compile command, each narrowing a
// probe that was answering the wrong question. Read as history now — the
// `__STDC_HOSTED__ && __has_include(<chrono>) && __has_include(<ratio>)`
// disjunct it describes is gone, because W2 removed discovery from every
// capability here. What survives is the finding: a header can be PRESENT,
// pass every probe, and still not provide the thing you need.

// `<chrono>` requires the EXPLICIT opt-in, not `__has_include`, and that is
// measured rather than stylistic.
//
// `__has_include(<chrono>)` is TRUE on the Zephyr arm-none-eabi toolchain --
// the file exists -- but under `-ffreestanding` libstdc++ ships it INCOMPLETE:
// `std::chrono::duration_cast` is absent, so `create_wall_timer` and `Rate`
// failed to compile on every Zephyr C++ image with
// `'duration_cast' is not a member of 'std::chrono'`. Replayed from that build
// dir's own recorded compile command, not inferred.
//
// So the idiom has a boundary worth stating: `__has_include` answers "does the
// header exist", which is the right question for `<memory>` (present or absent
// as a unit) and the WRONG one for `<chrono>` (present but hollowed out). Where
// a freestanding toolchain ships a partial header, only the consumer's own
// `NROS_CPP_STD` opt-in is a reliable signal. Issue 0112's class, one layer past
// where step A caught it.
// CORRECTED 2026-09-05. The `NROS_CPP_STD`-only gate above was measured on the
// Zephyr lane and never on a plain hosted one, and NOTHING THAT SHIPS DEFINES
// `NROS_CPP_STD`: it appears in no cmake module, no toolchain file, no build.rs
// and no Kconfig -- only in docs and in `check.just`'s own probe TUs. So the
// narrow gate did not merely tighten `<chrono>`; it removed `create_wall_timer`
// and `Rate` from EVERY shipped configuration, including the hosted one, where
// `examples/templates/cpp-port-minimal-publisher` calls `create_wall_timer` and
// is phase-417's own acceptance criterion.
//
// The right predicate needs BOTH halves, because the two lanes fail
// differently and neither test alone sees both:
//
//   * Zephyr / ThreadX-RV64 build `-nostdinc++` against a minimal libcpp that
//     has no `<chrono>` at all, so `__has_include` answers FALSE and is exactly
//     right there.
//   * FreeRTOS armcm3 builds `-ffreestanding` WITHOUT `-nostdinc++`, so
//     `__has_include(<chrono>)` answers TRUE against the host's own libstdc++
//     -- which then refuses to be used, because GCC 13 gates
//     `bits/requires_hosted.h` on `__STDC_HOSTED__` and `-ffreestanding` clears
//     it. That is the "present but hollow" case, and `__STDC_HOSTED__` is
//     precisely the flag that distinguishes it.
//
// `__STDC_HOSTED__` alone is still not enough -- CLAUDE.md's pitfall entry says
// so, and it is right: a hosted compiler can run `-nostdinc++` against Zephyr's
// minimal libcpp, where the macro is 1 and the header is absent. Neither test
// covers both lanes; the conjunction does.
//
// `NROS_CPP_STD` stays as an explicit override for a consumer who knows better
// than the probes, which is what the parity extractor and the docs use it for.
// CORRECTED AGAIN 2026-09-07, and this time the discriminator is the library's
// own feature-test macro rather than a guess about which RTOS ships what.
//
// The conjunction above is still not enough. Measured under the safety island's
// OWN recorded compile command (`-nostdinc++`, arm-zephyr-eabi, picolibc), not
// inferred:
//
//     __STDC_HOSTED__          1
//     __has_include(<chrono>)  TRUE
//     __cpp_lib_chrono         ABSENT
//
// So Zephyr does NOT merely "have no <chrono> at all" -- this toolchain ships
// one that is present and hollow, which is the case the note above attributes
// to FreeRTOS alone. Both halves of the conjunction answer yes and
// `duration_cast` is still missing, so `Rate`'s duration constructor failed to
// compile on the island exactly as it did before that note was written.
//
// `<ratio>` is the third test, and it is a PREREQUISITE rather than a proxy.
// `duration_cast<To>(duration<Rep, Period>)` is defined in terms of
// `std::ratio_divide` -- a duration's `Period` IS a `std::ratio` -- so a
// `<chrono>` shipped without `<ratio>` cannot provide `duration_cast`, and that
// is exactly the shape this toolchain ships. Measured on both sides:
//
//                        island (Zephyr)   hosted g++ -std=c++14
//   __has_include(<chrono>)   TRUE              TRUE
//   __has_include(<ratio>)    FALSE             TRUE
//
// `__cpp_lib_chrono` was tried first and is WRONG here: it is a C++17
// feature-test macro, so it is absent under `-std=c++14` even where the library
// is complete. `check-cpp` compiles the phase-417 probes at C++14 and caught
// that immediately -- gating on it removed `create_wall_timer` from the hosted
// build, which is the same over-tightening the note above records.
// `_GLIBCXX_CHRONO` also separates the two but is libstdc++'s private spelling;
// `__has_include` is the portable question.
#if defined(NROS_CPP_STD)
#include <chrono>
#define NROS_CPP_HAS_STD_CHRONO 1
#endif

#endif // NROS_CPP_STD_DETECT_HPP
