/* phase-454 W10 -- the DECLARED depth reaches the C compiler, and the three
 * cases it has to tell apart all COMPILE.
 *
 * This is the POSITIVE half of C's declared-QoS check.
 * `declared_qos_depth_probe.c` beside it is the negative half, and the negative
 * half is the one that matters: a check that has never failed is not a check.
 * Neither is worth anything alone -- an expected-failure compile cannot
 * distinguish "the _Static_assert fired" from "the file is not there", which is
 * why `just check c` asserts this TU compiles clean BEFORE it asserts the probe
 * does not.
 *
 * The declared table is the SAME committed artifact the C++ half compiles
 * against, `packages/api/nros-cpp/tests/compile/declared-qos-fixture/nros/`,
 * which is what `nros ws entity-inventory --output-header` renders and what
 * `the_committed_compile_fixture_is_what_this_emitter_renders` holds to the
 * emitter. One generated descriptor serves both languages -- that is the whole
 * claim of this wave -- so pointing both probes at one fixture is the honest
 * arrangement, not a shortcut. In a real build `nano_ros_node_register()`
 * writes that header into the component library's own include dir, for a C
 * component exactly as for a C++ one.
 *
 * Compiled with `-std=c11 -Wall -Wextra`.
 */
#include <nros/nros.h>

/* -- The LOOKUP itself, before any macro is involved -----------------------
 *
 * If these drift, every assertion below becomes vacuous while still compiling,
 * which is the failure mode this whole mechanism was written against. */

_Static_assert(NROS_DECLARED_QOS_COMPILE_TIME == 1,
               "the fixture table reached this TU AND this compiler folds the search. If it "
               "did not, every assertion below would hold trivially and `just check c` would "
               "be green over a mechanism that does nothing.");

_Static_assert(NROS_DECLARED_DEPTH("std_msgs::msg::dds_::Int32_", "/chatter") == 1,
               "the fixture declares sub:std_msgs/msg/Int32:/chatter@depth=1, and the "
               "DDS-mangled spelling is the key a generated C message header carries");

_Static_assert(NROS_DECLARED_DEPTH("std_msgs/msg/Int32", "/chatter") == 1,
               "the ROS spelling of the type is a key too -- a hand-written call site may "
               "write it instead of the DDS-mangled one");

_Static_assert(NROS_DECLARED_DEPTH("std_msgs::msg::dds_::Bool_", "/undeclared") ==
                   NROS_DECLARED_DEPTH_UNDECLARED,
               "an endpoint that declared no depth is UNDECLARED, which is -1 and not 0. A 0 "
               "here would make every call site look like a mismatch, and would let a size "
               "consumer read a queue of zero as an answer");

_Static_assert(NROS_DECLARED_DEPTH_UNDECLARED != 0,
               "absence must never be spelled the same as a depth");

_Static_assert(NROS_DECLARED_DEPTH("std_msgs::msg::dds_::Int32_", "/a_topic_nobody_declared") ==
                   NROS_DECLARED_DEPTH_UNDECLARED,
               "the table is keyed on the PAIR: a declared type on an undeclared topic is not "
               "a hit, or one declaration would size every topic that type is carried on");

/* -- The fill-in an undeclared endpoint gets ------------------------------- */

_Static_assert(NROS_DECLARED_DEPTH_OR(NROS_DECLARED_DEPTH("std_msgs/msg/Int32", "/chatter"), 10) ==
                   1,
               "where the contract states a depth, that is the number to check against");

_Static_assert(NROS_DECLARED_DEPTH_OR(NROS_DECLARED_DEPTH("std_msgs/msg/Bool", "/undeclared"),
                                      10) == 10,
               "and where it states nothing there is nothing to disagree with, so the passed "
               "depth is compared with itself and the assertion holds");

/* -- The macro, in the shape a C node's setup writes ----------------------- */

static void on_msg(const uint8_t* data, size_t len, void* ctx) {
    (void)data;
    (void)len;
    (void)ctx;
}

/* The DEPTH a C call site states. A literal in a macro argument is the one
 * shape that survives to `_Static_assert`: `nros_qos_default()`'s `.depth` is a
 * struct field, and `NROS_QOS_DEFAULT` is an `extern const` object resolved at
 * link time, so neither is a constant expression. Naming the number once, as a
 * macro, is what lets the build check it -- and it is the same discipline the
 * C++ side gets for free from `constexpr nros::QoS(1)`. */
#define CHATTER_DEPTH 1

nros_ret_t nros_c_declared_qos_compile_test(struct nros_subscription_t* sub,
                                            const struct nros_node_t* node,
                                            const struct nros_message_type_t* type_info);
nros_ret_t nros_c_declared_qos_compile_test(struct nros_subscription_t* sub,
                                            const struct nros_node_t* node,
                                            const struct nros_message_type_t* type_info) {
    /* Mode 1: the code states the depth and the declaration agrees. */
    struct nros_qos_t qos = NROS_QOS_DEFAULT;
    qos.depth = CHATTER_DEPTH;
    NROS_ASSERT_DECLARED_DEPTH("std_msgs::msg::dds_::Int32_", "/chatter", CHATTER_DEPTH,
                               "\"/chatter\"");
    nros_ret_t rc =
        nros_subscription_init_with_qos(sub, node, type_info, "/chatter", on_msg, NULL, &qos);
    if (rc != NROS_RET_OK) {
        return rc;
    }

    /* An endpoint the contract declares nothing for: not an error, and not a
     * default anybody guessed -- whatever the call site already passed. */
    struct nros_qos_t other = NROS_QOS_DEFAULT;
    other.depth = 4;
    NROS_ASSERT_DECLARED_DEPTH("std_msgs::msg::dds_::Bool_", "/undeclared", 4, "\"/undeclared\"");
    rc = nros_subscription_init_with_qos(sub, node, type_info, "/undeclared", on_msg, NULL, &other);
    if (rc != NROS_RET_OK) {
        return rc;
    }

    /* Two subscriptions may be checked in one function: the diagnostic carrier
     * is keyed on __LINE__, so they do not collide with each other and report a
     * mismatch between two unrelated topics. */

    /* The RUNTIME form, for a topic that is not a constant expression. Same
     * rows, same answer; this is what a call site whose topic is built at run
     * time gets instead of the build failure. */
    const char* topic = "/chatter";
    if (nros_declared_depth(topic, topic) != NROS_DECLARED_DEPTH_UNDECLARED) {
        return NROS_RET_ERROR;
    }
    if (nros_declared_depth("std_msgs/msg/Int32", topic) != 1) {
        return NROS_RET_ERROR;
    }
    if (!nros_declared_depth_agrees("std_msgs/msg/Int32", topic, 1)) {
        return NROS_RET_ERROR;
    }
    if (nros_declared_depth_agrees("std_msgs/msg/Int32", topic, 10)) {
        return NROS_RET_ERROR;
    }
    if (!nros_declared_depth_agrees("std_msgs/msg/Bool", "/undeclared", 4)) {
        return NROS_RET_ERROR;
    }
    return NROS_RET_OK;
}
