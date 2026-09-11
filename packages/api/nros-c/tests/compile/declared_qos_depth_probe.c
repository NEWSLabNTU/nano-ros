/* phase-454 W10 -- the NEGATIVE half: this TU MUST NOT COMPILE.
 *
 * The fixture beside the C++ probe declares
 * `sub:std_msgs/msg/Int32:/chatter@depth=1`. The call site below passes depth
 * 10. One subscription, two different depths, and depth is a multiplier on the
 * executor arena -- so the build has to stop.
 *
 * WHY THIS FILE EXISTS AT ALL. A compile-time check is the easiest kind of
 * mechanism to leave vacuous, because a table that never matches, a macro arm
 * never selected and a topic spelled differently in the declaration all look
 * exactly like "no mismatch here". The only evidence that the check is
 * REACHABLE is a case where it fires, so here is one: `just check c` compiles
 * it expecting failure AND greps the diagnostic for the topic and both numbers,
 * because a rejection for the wrong reason (a typo, a missing include) would
 * otherwise read as a pass.
 *
 * The positive TU beside this one is asserted to compile clean FIRST, for the
 * same reason.
 *
 * What a reader should see:
 *
 *   error: static assertion failed: "nros: the QoS depth passed for topic
 *          "/chatter" disagrees with the depth declared for that topic ..."
 *   error: conflicting types for 'nros_declared_depth_vs_passed_at_line_NN';
 *          have 'char[10]'
 *   note: previous declaration ... with type 'char[1]'
 *
 * The first names the TOPIC, which only the macro can supply; the second names
 * BOTH NUMBERS, which only a type can, since a `_Static_assert` message is a
 * string literal and cannot interpolate an `int`. C++ gets the same two halves
 * out of a `static_assert` and a template instantiation.
 */
#include <nros/nros.h>

void nros_c_declared_qos_probe(void);
void nros_c_declared_qos_probe(void) {
    /* DECLARED depth 1. PASSED depth 10. This line is the whole test. */
    NROS_ASSERT_DECLARED_DEPTH("std_msgs::msg::dds_::Int32_", "/chatter", 10, "\"/chatter\"");
}
