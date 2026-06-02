// Phase 212.M.10 fixture — Entry pkg main. Forces a link reference to
// each Component pkg marker symbol so the STATIC libs survive --gc.
#include <cstdio>

extern "C" int nros_fixture_talker_marker();
extern "C" int nros_fixture_listener_marker();

int main() {
    int rc = nros_fixture_talker_marker() | nros_fixture_listener_marker();
    std::puts("demo_entry");
    return rc;
}
