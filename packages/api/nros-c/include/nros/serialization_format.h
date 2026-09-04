/**
 * @file serialization_format.h
 * @ingroup grp_types
 * @brief RFC-0088 D5 — the linked backend's serialization format, as a
 *        compile-time fact the C API can assert on.
 *
 * ROS 2 asks `rmw_get_serialization_format()` at run time because it resolves
 * its typesupport through `dlopen`. nano-ros does not `dlopen`, so the answer is
 * already known when the image is compiled: exactly one backend is linked, and
 * one backend speaks exactly one encoding.
 *
 * `NROS_SERIALIZATION_FORMAT_ID` is generated — it is `cbindgen` output in
 * `nros/nros_generated.h`, lowered from `nros_c::constants`, whose value is
 * proven equal to `nros_node::session::IMAGE_SERIALIZATION_FORMAT_ID` by a
 * `const _` in that module. So the macro cannot silently disagree with the
 * backend this image links: a backend whose format is not the one mirrored
 * there fails the Rust build with a message naming the drift.
 *
 * `NROS_SERIALIZATION_FORMAT` is its cross-image identity string, derived here
 * from the discriminant. RFC-0088 D2 is the reason for that direction: the
 * **string** is the identity that crosses image boundaries, and the **`u8` is
 * image-local** — never persist it, never compare it against a value another
 * image produced.
 *
 * ## Scope — a bridge image must not use these macros
 *
 * A bridge image (`Executor::open_multi`) links two backends and therefore has
 * no single answer, so neither macro means anything there; such an image asks
 * per session instead, with `nros_node_get_serialization_format()`.
 * `scripts/check-format-macro-scope.py` refuses a bridge-linked translation
 * unit that references either macro.
 *
 * Copyright 2026 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_SERIALIZATION_FORMAT_H
#define NROS_SERIALIZATION_FORMAT_H

#include "nros/nros_generated.h"

/**
 * Portable compile-time assertion.
 *
 * Spelled once here because the generated message headers are compiled as C
 * *and* included from C++ (they carry their own `extern "C"` block), and
 * `_Static_assert` is not a C++ keyword. C23 spells the C keyword
 * `static_assert` too; C11 spells it `_Static_assert`; the C99 fallback is the
 * negative-array-size idiom, so a freestanding pre-C11 toolchain still gets the
 * diagnostic rather than silently skipping the check.
 */
#if defined(__cplusplus)
#define NROS_STATIC_ASSERT(cond, msg) static_assert(cond, msg)
#elif defined(__STDC_VERSION__) && __STDC_VERSION__ >= 202311L
#define NROS_STATIC_ASSERT(cond, msg) static_assert(cond, msg)
#elif defined(__STDC_VERSION__) && __STDC_VERSION__ >= 201112L
#define NROS_STATIC_ASSERT(cond, msg) _Static_assert(cond, msg)
#else
/* Pre-C11 fallback: a negative array size. The tag must be unique within the
   translation unit, and `__LINE__` is NOT — two generated message headers
   included in one TU collide whenever the assertion lands on the same line
   number in both. `__COUNTER__` (gcc, clang, armcc) is unique per expansion;
   `__LINE__` is the last resort for a compiler with neither. */
#define NROS_STATIC_ASSERT_CAT_(a, b) a##b
#define NROS_STATIC_ASSERT_CAT(a, b) NROS_STATIC_ASSERT_CAT_(a, b)
#ifdef __COUNTER__
#define NROS_STATIC_ASSERT_TAG_ __COUNTER__
#else
#define NROS_STATIC_ASSERT_TAG_ __LINE__
#endif
#define NROS_STATIC_ASSERT(cond, msg)                                                              \
    typedef char NROS_STATIC_ASSERT_CAT(nros_static_assert_,                                       \
                                        NROS_STATIC_ASSERT_TAG_)[(cond) ? 1 : -1]
#endif

/* RFC-0088 D2 — reserved discriminants for the in-tree formats. Low values for
   readability, and nothing more: the allocation is per image, so these are the
   values THIS image's `nros_serdes::format::SerializationFormatId` uses, not a
   registry any other image is bound by. */
#define NROS_SERIALIZATION_FORMAT_ID_CDR 1
#define NROS_SERIALIZATION_FORMAT_ID_UORB 2

#ifndef NROS_SERIALIZATION_FORMAT_ID
#error                                                                                             \
    "NROS_SERIALIZATION_FORMAT_ID is missing — <nros/nros_generated.h> did not supply it. Regenerate the committed cbindgen headers (`cargo run -p nros-cbindgen-headers`)."
#endif

/**
 * Cross-image identity of `NROS_SERIALIZATION_FORMAT_ID` (RFC-0088 D2).
 *
 * Derived from the discriminant rather than emitted beside it: `cbindgen` maps
 * no Rust `&str` to a C constant, so a second generated macro would be a second
 * authored spelling with nothing tying the two together. One generated number
 * and one table is the shape that cannot drift into disagreeing with itself.
 */
#if NROS_SERIALIZATION_FORMAT_ID == NROS_SERIALIZATION_FORMAT_ID_CDR
#define NROS_SERIALIZATION_FORMAT "cdr"
#elif NROS_SERIALIZATION_FORMAT_ID == NROS_SERIALIZATION_FORMAT_ID_UORB
#define NROS_SERIALIZATION_FORMAT "uorb"
#else
#error                                                                                             \
    "NROS_SERIALIZATION_FORMAT_ID names a format this header has no name for — add its row here and in `nros_serdes::format::SerializationFormatId` together (RFC-0088 D2)."
#endif

/**
 * Assert that a message type's serialization format is the one the linked
 * backend speaks. Emitted once per generated message header.
 *
 * @param msg_format_id  the type's own `NROS_MSG_FORMAT_ID_<type>` macro
 * @param type_literal   the type's name, as a string literal, so the
 *                       diagnostic names the message rather than the header
 */
#define NROS_ASSERT_MESSAGE_FORMAT(msg_format_id, type_literal)                                    \
    NROS_STATIC_ASSERT((msg_format_id) == NROS_SERIALIZATION_FORMAT_ID,                            \
                       "RFC-0088: " type_literal                                                   \
                       " is not encoded in the format the linked backend speaks "                  \
                       "(NROS_SERIALIZATION_FORMAT) — one image, one backend, one encoding")

#endif /* NROS_SERIALIZATION_FORMAT_H */
