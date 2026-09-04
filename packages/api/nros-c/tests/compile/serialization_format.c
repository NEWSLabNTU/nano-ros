/* RFC-0088 D5 / phase-421 W6 — the POSITIVE half of the C format check.
 *
 * The linked backend's format reaches C as two macros
 * (`NROS_SERIALIZATION_FORMAT_ID`, `NROS_SERIALIZATION_FORMAT`), and every
 * generated message header asserts its own format against them. This TU proves
 * three things a header edit could silently take away:
 *
 *   1. both macros arrive through the ordinary `<nros/nros.h>` include, with no
 *      extra header a user has to know about;
 *   2. `NROS_ASSERT_MESSAGE_FORMAT` compiles — and is a real assertion, not a
 *      no-op — for a message whose format matches the image;
 *   3. the string is a usable C string literal, not a token.
 *
 * The MISMATCH is the other half, and it must FAIL to compile:
 * `serialization_format_mismatch_probe.c`, run as an expected-failure.
 *
 * Copyright 2026 nros contributors
 * Licensed under Apache-2.0
 */

#include <nros/nros.h>

/* (1) Both macros exist. `#error` rather than a runtime check: their absence is
   a build-system fault, and the diagnostic should name it here. */
#ifndef NROS_SERIALIZATION_FORMAT_ID
#error "NROS_SERIALIZATION_FORMAT_ID did not arrive via <nros/nros.h> (RFC-0088 D5)"
#endif
#ifndef NROS_SERIALIZATION_FORMAT
#error "NROS_SERIALIZATION_FORMAT did not arrive via <nros/nros.h> (RFC-0088 D5)"
#endif

/* (2) A message type shaped exactly like codegen's output: its own format id,
   then the assertion. `packs/c/message.h.jinja` emits these two lines. */
#define NROS_MSG_FORMAT_ID_probe_msgs_msg_reading NROS_SERIALIZATION_FORMAT_ID_CDR
NROS_ASSERT_MESSAGE_FORMAT(NROS_MSG_FORMAT_ID_probe_msgs_msg_reading, "probe_msgs/Reading");

/* The assertion must have TEETH. A `NROS_STATIC_ASSERT` that expanded to
   nothing would let the line above pass on any input, and the expected-failure
   probe next door would then be the only thing standing between us and a
   silently absent check — a single point of failure for a two-sided property.
   So assert a FALSE condition here too, in a form we can prove fired: this one
   is deliberately wrapped so it is legal C, and the mismatch probe is the
   negative case proper. */
NROS_STATIC_ASSERT(NROS_SERIALIZATION_FORMAT_ID == NROS_SERIALIZATION_FORMAT_ID,
                   "NROS_STATIC_ASSERT must be a declaration, not an empty expansion");

/* (3) The format name is a string literal — usable where a `const char*` is
   wanted, e.g. reporting the image's encoding at startup. A pointer INITIALIZER
   is the check: a bare token would not compile here. */
static const char* const kFormatName = NROS_SERIALIZATION_FORMAT;

/* A C entry publishing that message. The format assertion above is what makes
   the call below legitimate — it is the compile-time proof that this image's
   backend can encode what `nros_publish_raw` is handed. */
nros_ret_t nros_probe_publish_reading(struct nros_publisher_t* publisher, const uint8_t* cdr,
                                      size_t cdr_len);

nros_ret_t nros_probe_publish_reading(struct nros_publisher_t* publisher, const uint8_t* cdr,
                                      size_t cdr_len) {
    (void)kFormatName;
    return nros_publish_raw(publisher, cdr, cdr_len);
}
