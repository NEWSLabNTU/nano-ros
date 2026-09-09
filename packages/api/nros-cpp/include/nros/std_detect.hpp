// nros-cpp: standard-library capability detection — THE one site
// Freestanding C++ — this header includes nothing a minimal libcpp lacks
// unless it has first established that the capability is both PRESENT and
// USABLE.

/**
 * @file std_detect.hpp
 * @brief The single definition of the `NROS_CPP_HAS_*` capability macros.
 *
 * Every other nros-cpp header includes this one and TESTS the macros. None of
 * them defines one.
 *
 * Before phase-438 W1 this block was hand-copied FIFTEEN times across eleven
 * headers — the "second spelling rather than one shared helper" antipattern
 * CLAUDE.md records for the Zephyr unset-variable guard (#282 -> #326). The
 * predicate below is the survivor of three separate corrections (issues 0112,
 * 1187, 1240), and each one had to be applied fifteen times by hand. Issue
 * 1240's own fix ships a sweep command for the next person precisely because
 * there was no single site to change:
 *
 *     grep -rn '__has_include' packages/api/nros-cpp/include/nros/*.hpp
 *
 * There is now. That sweep is this file.
 *
 * Two things consolidation fixes beyond the repetition:
 *
 *   1. `nros.hpp` USED four of these macros and DEFINED none, so which of them
 *      held depended on `publisher.hpp` / `options.hpp` / `subscription.hpp`
 *      having been included first. It was the only header in that state, and
 *      the failure mode is issue 0135's: two TUs of one image disagreeing
 *      about a capability, hence about a layout.
 *   2. The rationale drifted. Eleven copies carried a comment saying the test
 *      is `__has_include` "rather than `__STDC_HOSTED__`, issue 0112" — and
 *      issue 0112 says no such thing. Its Resolution chose `NROS_CPP_STD` over
 *      `__STDC_HOSTED__`; it never chose `__has_include`, which arrived later
 *      in commit `acf213871`. Those comments are gone; this is the one place
 *      the reasoning lives, so it can only drift once.
 *
 * ## The widening, stated
 *
 * A header that needs only `<memory>` now also gets the other five wherever
 * they are usable. That is what makes one site possible, and it is safe
 * because the predicate is evaluated per capability: a toolchain that has
 * `<memory>` and not `<sstream>` still gets `NROS_CPP_HAS_SHARED_PTR` and not
 * `NROS_CPP_HAS_STD_SSTREAM`. What changes is only that every TU including any
 * nros-cpp header now agrees about all six, which is the issue-0135 fix above
 * rather than a cost.
 */

#ifndef NROS_CPP_STD_DETECT_HPP
#define NROS_CPP_STD_DETECT_HPP

// --- Why the predicate is a DISJUNCTION over a CONJUNCTION --------------------
//
// Moved here verbatim from `publisher.hpp`, which carried it for the other ten
// headers under the note "the rationale lives here". It still does; the file
// changed.
//
// Gate on the declared std flavour, else ASK THE COMPILER — with BOTH probes,
// because each one alone gives a wrong answer on a lane the other gets right.
//
// `__STDC_HOSTED__` alone is the WRONG question (issue 0112), and measurably
// so: the Zephyr XRCE C++ leaves compile with `-fno-freestanding -nostdinc++`,
// i.e. they read HOSTED while having no `<memory>` at all, and the aarch64
// workspace leaf has `-nostdinc++` with no `-ffreestanding` either. Only
// `__has_include` sees that.
//
// `__has_include` alone is ALSO wrong, and this half was missing until issue
// 1240. It answers "does the header FILE exist" — which is TRUE under
// `-ffreestanding` against a FULL libstdc++, whose `<memory>` / `<string>` /
// `<vector>` / `<sstream>` open with
//
//     #error "This header is not available in freestanding mode."
//
// GCC 16 made that unmissable: `just check cpp`'s own `-ffreestanding` probe
// stopped compiling, ~200 errors deep inside `/usr/include/c++/16` and none in
// our code, and the `shared_ptr does not name a template type` reports it ended
// with were a CASCADE — `timer.hpp` includes `<memory>` and always did. The
// same shape had been failing quietly on the FreeRTOS lane's arm-none-eabi 13.2
// for longer (issue 1187: 19 of 45 headers, 17 of them entered through
// `log.hpp`'s `<string>`). `__STDC_HOSTED__` is the only probe that separates
// "present" from "present and usable"; `nros.hpp` measured the same thing for
// `<chrono>` two days earlier and this is its class.
//
// So: `NROS_CPP_STD` (the explicit consumer opt-in, which nothing that ships
// defines — do not gate on it ALONE, that removes the surface from the hosted
// build too) OR the conjunction. Gated by
// `scripts/check-cpp-freestanding-includes.sh`, which since 1240 rejects an
// `#if` that names `NROS_CPP_STD` while its live arm asks only
// `__has_include`.
//

// --- The five uniform capabilities -------------------------------------------
//
// Uniform in the literal sense: before consolidation these fourteen blocks
// normalised to one block text, with the header name and the macro name
// consistent at all three positions in every instance. `<chrono>` below is the
// fifteenth and it does NOT normalise into these.

#if defined(NROS_CPP_STD) || (defined(__STDC_HOSTED__) && __STDC_HOSTED__ && __has_include(<memory>))
#include <memory>
#define NROS_CPP_HAS_SHARED_PTR 1
#endif

#if defined(NROS_CPP_STD) || (defined(__STDC_HOSTED__) && __STDC_HOSTED__ && __has_include(<string>))
#include <string>
#define NROS_CPP_HAS_STD_STRING 1
#endif

#if defined(NROS_CPP_STD) || (defined(__STDC_HOSTED__) && __STDC_HOSTED__ && __has_include(<vector>))
#include <vector>
#define NROS_CPP_HAS_STD_VECTOR 1
#endif

#if defined(NROS_CPP_STD) || (defined(__STDC_HOSTED__) && __STDC_HOSTED__ && __has_include(<functional>))
#include <functional>
#define NROS_CPP_HAS_STD_FUNCTION 1
#endif

// `<sstream>` is the `RCLCPP_*_STREAM` family only. Worth knowing where it
// sits: `log.hpp` pulls it, `qos.hpp` includes `log.hpp`, and every entity
// header includes `qos.hpp` — so a stream-formatting convenience is on the
// transitive include path of every freestanding TU in the API.
#if defined(NROS_CPP_STD) || (defined(__STDC_HOSTED__) && __STDC_HOSTED__ && __has_include(<sstream>))
#include <sstream>
#define NROS_CPP_HAS_STD_SSTREAM 1
#endif

// --- `<chrono>`: the one predicate that is NOT the uniform block --------------
//
// This block reached the correct shape first, and the hard way — the comment
// below records three successive corrections, each measured against a build's
// own recorded compile command, and issue 1240's fix is explicitly that
// reasoning generalised to the other fourteen. Moved here verbatim, comment
// included, because the comment is the evidence rather than decoration.
//
// It differs in one way that is load-bearing and must not be normalised away:
// `<ratio>` is a SECOND probe used as a PREREQUISITE rather than a proxy.
// `duration_cast<To>(duration<Rep, Period>)` is defined in terms of
// `std::ratio_divide` — a duration's `Period` IS a `std::ratio` — so a
// `<chrono>` shipped without `<ratio>` cannot provide `duration_cast`, which is
// exactly what the safety island's toolchain ships.

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
#if defined(NROS_CPP_STD) || (defined(__STDC_HOSTED__) && __STDC_HOSTED__ && __has_include(<chrono>) && __has_include(<ratio>))
#include <chrono>
#define NROS_CPP_HAS_STD_CHRONO 1
#endif

#endif // NROS_CPP_STD_DETECT_HPP
