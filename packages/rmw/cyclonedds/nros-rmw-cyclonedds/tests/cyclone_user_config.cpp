// phase-206 W2 — the bringup's own Cyclone config reaches the backend, and the
// SAME BYTES reach it on a hosted build and on an RTOS build.
//
// The gap this closes: on a hosted ROS 2 system a user sets `CYCLONEDDS_URI` to
// an XML file and application code knows nothing about NICs. On an RTOS there
// is no environment and no filesystem — `env_lookup` returns `nullptr` on a
// freestanding target — so that rung is structurally dead and the user has no
// way in at all. The file is therefore BAKED, and this test pins the two claims
// that make baking worth anything:
//
//   1. the bytes arrive VERBATIM. Not re-spelled, not re-quoted, not parsed by
//      nano-ros — byte-identical to the file on disk, so a user's existing ROS 2
//      knowledge transfers and their document is reviewable as itself.
//   2. the baked config COMPOSES rather than replaces (phase-206 W1), so
//      shipping one does not silently cost the baked baseline.
//
// The negative control is claim 1's own inverse: an image with no bringup config
// must compose exactly as if this feature did not exist. Without that, a bug
// that made every image carry a stray fragment would still pass.

#define NROS_RMW_CYCLONEDDS_TEST_BASELINE 1
#include "cyclone_config.hpp"
#include "user_config.hpp"

#include <cstdio>
#include <cstring>
#include <string>

namespace {

int g_failed = 0;

void check(bool ok, const char* what) {
    if (!ok) {
        std::printf("FAIL: %s\n", what);
        ++g_failed;
    }
}

// The fixture, as bytes, independent of the generator — if the two disagree the
// generator mangled something, which is exactly what "verbatim" forbids.
const char* fixture_path() {
#ifdef NROS_W2_FIXTURE_PATH
    return NROS_W2_FIXTURE_PATH;
#else
    return nullptr;
#endif
}

std::string read_file(const char* path) {
    std::FILE* f = std::fopen(path, "rb");
    if (f == nullptr) {
        return std::string();
    }
    std::string out;
    char buf[4096];
    size_t n;
    while ((n = std::fread(buf, 1, sizeof(buf), f)) > 0) {
        out.append(buf, n);
    }
    std::fclose(f);
    return out;
}

}  // namespace

int main() {
    using namespace nros_rmw_cyclonedds;

    const char* path = fixture_path();
    if (path == nullptr) {
        std::printf("FAIL: NROS_W2_FIXTURE_PATH was not defined; the test cannot\n"
                    "      compare the baked bytes against the file they came from.\n");
        return 1;
    }

    const std::string on_disk = read_file(path);
    if (on_disk.empty()) {
        std::printf("FAIL: fixture %s is empty or unreadable — the comparison below\n"
                    "      would pass vacuously against an empty baked config.\n", path);
        return 1;
    }

    // 1. VERBATIM. Byte-for-byte, including the comment and the indentation:
    //    the point of baking a file rather than a nano-ros re-spelling is that
    //    the user's document survives intact.
    check(has_user_cyclone_config(),
          "the image carries a baked user config (the bake did not run?)");
    check(on_disk == std::string(kUserCycloneConfig),
          "baked bytes are byte-identical to the file on disk");

    // The elements a user came here for. Asserted by CONTENT rather than by
    // length, so a generator that dropped the XML comment would still be caught
    // by the byte compare above while this says which part matters.
    check(std::string(kUserCycloneConfig).find("<NetworkInterface") != std::string::npos,
          "the user's <NetworkInterface> element survives the bake");
    check(std::string(kUserCycloneConfig).find("<Peer Address=\"127.0.0.1\"/>") != std::string::npos,
          "the user's <Peer> element survives the bake, quotes and all");

    // 2. COMPOSES, not replaces. This is W1's guarantee applied to W2's new
    //    fragment: the four sources in precedence order, baseline first.
    const char* frags[4] = {kEmbeddedCycloneConfig, "", kUserCycloneConfig, ""};
    char composed[kCycloneConfigMax];
    check(compose_cyclone_config(composed, sizeof(composed), frags, 4),
          "baseline + baked user config compose within the buffer");
    const std::string c(composed);
    check(c.find("<StackSize>64 KiB</StackSize>") != std::string::npos,
          "composition: the baseline's 64 KiB thread stacks survive a baked config");
    check(c.find("<NetworkInterface") != std::string::npos,
          "composition: the user's interface binding is present");
    check(c.rfind(kEmbeddedCycloneConfig, 0) == 0,
          "composition: the baseline comes FIRST, so the user's tokens override it");

    // NEGATIVE CONTROL. An image with no bringup config must compose exactly the
    // baseline — no stray separator, no empty token. `compose_cyclone_config`
    // skips empties, and this is what proves it rather than assuming it.
    const char* none[4] = {kEmbeddedCycloneConfig, "", "", ""};
    char control[kCycloneConfigMax];
    check(compose_cyclone_config(control, sizeof(control), none, 4),
          "control: an image with no user config composes");
    check(std::string(control) == std::string(kEmbeddedCycloneConfig),
          "control: with no user config the result IS the baseline, unchanged");

    if (g_failed != 0) {
        std::printf("%d check(s) failed\n", g_failed);
        return 1;
    }
    std::printf("phase-206 W2: baked config is verbatim and composes; control clean\n");
    return 0;
}
