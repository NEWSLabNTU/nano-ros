// Phase 117.X.2 unit test for the topic-prefix helper.
//
// Verifies the four cases that matter for stock-RMW interop:
//   1. plain name + prefix → "<prefix>/<name>"
//   2. already-prefixed input → unchanged (idempotent)
//   3. NROS_RMW_CYCLONEDDS_SKIP_PREFIX=1 in env → unchanged
//   4. overflow → returns false

#include <cstdio>
#include <cstdlib>
#include <cstring>

#include "topic_prefix.hpp"

using nros_rmw_cyclonedds::topic_prefix::apply;
using nros_rmw_cyclonedds::topic_prefix::already_prefixed;

static int fail_count = 0;
#define EXPECT(cond, msg) \
    do { if (!(cond)) { \
        std::fprintf(stderr, "FAIL: %s (line %d)\n", msg, __LINE__); \
        ++fail_count; } } while (0)
#define EXPECT_STR(actual, expected) \
    do { if (std::strcmp((actual), (expected)) != 0) { \
        std::fprintf(stderr, "FAIL: got '%s' expected '%s' (line %d)\n", \
                     (actual), (expected), __LINE__); \
        ++fail_count; } } while (0)

int main() {
    char buf[64];

    // Case 1: plain name + rt prefix.
    EXPECT(apply("chatter", "rt", buf, sizeof(buf)), "apply rt/chatter");
    EXPECT_STR(buf, "rt/chatter");

    // Case 1b: empty name still prefixes (degenerate but consistent).
    EXPECT(apply("", "rt", buf, sizeof(buf)), "apply rt empty");
    EXPECT_STR(buf, "rt/");

    // Case 1c: long-ish name + service prefix.
    EXPECT(apply("add_two_intsRequest", "rq", buf, sizeof(buf)),
           "apply rq service");
    EXPECT_STR(buf, "rq/add_two_intsRequest");

    // Case 2: already-prefixed inputs detected.
    EXPECT(already_prefixed("rt/chatter"), "rt/ already prefixed");
    EXPECT(already_prefixed("rq/foo"),     "rq/ already prefixed");
    EXPECT(already_prefixed("rr/foo"),     "rr/ already prefixed");
    EXPECT(already_prefixed("rs/foo"),     "rs/ already prefixed");
    EXPECT(!already_prefixed("chatter"),   "plain not prefixed");
    EXPECT(!already_prefixed("r/foo"),     "r/ alone not prefixed");
    EXPECT(!already_prefixed("ra/foo"),    "ra/ not prefixed");
    EXPECT(!already_prefixed(""),          "empty not prefixed");
    EXPECT(!already_prefixed(nullptr),     "null not prefixed");

    // Case 2b: apply on already-prefixed input passes through.
    EXPECT(apply("rt/chatter", "rt", buf, sizeof(buf)), "idempotent rt");
    EXPECT_STR(buf, "rt/chatter");
    EXPECT(apply("rq/already", "rt", buf, sizeof(buf)),
           "any-already-prefix passes through (no double prefix)");
    EXPECT_STR(buf, "rq/already");

    // Case 3: env-opt-out.
    EXPECT(setenv("NROS_RMW_CYCLONEDDS_SKIP_PREFIX", "1", 1) == 0, "setenv");
    EXPECT(apply("chatter", "rt", buf, sizeof(buf)), "apply with skip env");
    EXPECT_STR(buf, "chatter");
    EXPECT(unsetenv("NROS_RMW_CYCLONEDDS_SKIP_PREFIX") == 0, "unsetenv");

    // Case 3b: env=0 doesn't skip.
    EXPECT(setenv("NROS_RMW_CYCLONEDDS_SKIP_PREFIX", "0", 1) == 0, "setenv 0");
    EXPECT(apply("chatter", "rt", buf, sizeof(buf)), "env=0 doesn't skip");
    EXPECT_STR(buf, "rt/chatter");
    EXPECT(unsetenv("NROS_RMW_CYCLONEDDS_SKIP_PREFIX") == 0, "unsetenv");

    // Case 4: overflow.
    char tiny[4];
    EXPECT(!apply("chatter", "rt", tiny, sizeof(tiny)), "overflow");

    if (fail_count == 0) {
        std::printf("OK\n");
        return 0;
    }
    std::fprintf(stderr, "%d failure(s)\n", fail_count);
    return 1;
}
