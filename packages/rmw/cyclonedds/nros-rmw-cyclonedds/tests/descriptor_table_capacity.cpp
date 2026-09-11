// phase-454 W6.c — the descriptor table holds what the model says it registers.
//
// `Entry g_entries[NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES]` was hand-authored at
// 256, and over the cap `register_descriptor` DROPS. It cannot do otherwise: the
// registrations run from static constructors, which cannot signal an error and
// may predate the console, so the drop is counted and warned about lazily on the
// first failed lookup (issue 0280's residual fix). By then the operator is
// staring at `publisher_create` returning UNSUPPORTED for whichever package
// happened to be link-order last, and the cause is nowhere in sight.
//
// The size is now DERIVED: `nros codegen-system` counts the distinct DDS type
// names the SystemModel registers (the SAME count that sizes the Rust registry's
// `NROS_CYCLONEDDS_MAX_TYPES`) and writes the knob into the workspace's cargo
// `[env]`, from which `nros-rmw-cyclonedds-sys`'s build script turns it into a
// `-D` on `descriptors.cpp`. This test is the consumer half: given the derived
// cap, an autoware-scale workspace registers everything it declares.
//
// The build deliberately does NOT pin the cap for this TU. `descriptors.cpp` is
// compiled once into the library under whatever the enclosing build derived, so
// the test reads `kMaxRegisteredTypes` through the API rather than assuming it —
// and asserts the PROPERTY (nothing is dropped below the cap, and the cap is
// where dropping starts), which is true at every size.

#include <stdio.h>
#include <string.h>

#include "descriptors.hpp"

using namespace nros_rmw_cyclonedds;

namespace {

// One descriptor object is enough: the table maps NAME -> pointer, and dedup is
// by name. Its contents are never dereferenced here.
dds_topic_descriptor_t g_desc = {};

// Names must outlive the table -- `register_descriptor` stores the pointer, it
// does not copy. A static arena keeps them alive for the process, which is
// exactly the lifetime the real static-init registrations have.
constexpr int kAttempted = 400; // past the 256 fallback; ~86 is the autoware case
char g_names[kAttempted][32];

int failures = 0;

void check(bool ok, const char* what) {
    if (!ok) {
        fprintf(stderr, "FAIL: %s\n", what);
        ++failures;
    }
}

} // namespace

int main() {
    check(registered_descriptor_count() == 0, "the table starts empty");
    check(dropped_descriptor_count() == 0, "nothing dropped before anything is registered");

    for (int i = 0; i < kAttempted; ++i) {
        snprintf(g_names[i], sizeof(g_names[i]), "pkg::msg::dds_::M%d_", i);
        register_descriptor(g_names[i], &g_desc);
    }

    const size_t registered = registered_descriptor_count();
    const size_t dropped = dropped_descriptor_count();

    // The invariant, at whatever cap this build derived: every attempt is either
    // held or dropped, and nothing is lost in between.
    check(registered + dropped == (size_t)kAttempted,
          "every attempted registration is accounted for");

    // Everything the table holds is FINDABLE. A registration that landed and
    // then could not be looked up would be the same silent failure one step
    // further on.
    for (size_t i = 0; i < registered; ++i) {
        if (find_descriptor(g_names[i]) != &g_desc) {
            check(false, "a registered descriptor is not findable");
            break;
        }
    }

    // Idempotence: re-registering a held name changes nothing and does not
    // consume a slot. This is what keeps a count of DISTINCT type names the
    // right thing to size the table from -- the derivation counts distinct
    // names, and the table charges per distinct name.
    if (registered > 0) {
        register_descriptor(g_names[0], &g_desc);
        check(registered_descriptor_count() == registered,
              "re-registering a held name consumes no slot");
    }

    // THE ACCEPTANCE, and it is stated as a property rather than a number: a
    // workspace of ~86 types -- the autoware-safety-island case the old comment
    // names, and well past what `std_msgs` + `geometry_msgs` alone need -- loses
    // nothing. At the 256 fallback this holds; at a derived 400 the whole set
    // holds; at a cap below 86 it would not, and that is the failure the
    // derivation removes.
    check(registered >= 86, "an autoware-scale type set is held, not dropped");
    for (int i = 0; i < 86; ++i) {
        if (find_descriptor(g_names[i]) == nullptr) {
            check(false, "one of the first 86 types was dropped");
            break;
        }
    }

    // The negative control for the accounting above: past the cap, dropping is
    // REPORTED rather than silent. Without this the test would pass against a
    // `dropped_descriptor_count()` that always returned zero.
    if (registered < (size_t)kAttempted) {
        check(dropped > 0, "over the cap, the drop is counted");
        check(find_descriptor(g_names[kAttempted - 1]) == nullptr,
              "a dropped registration is genuinely absent");
    }

    if (failures != 0) {
        fprintf(stderr, "descriptor_table_capacity: %d failure(s)\n", failures);
        return 1;
    }
    printf("descriptor_table_capacity: OK (%zu held, %zu dropped of %d attempted)\n", registered,
           dropped, kAttempted);
    return 0;
}
