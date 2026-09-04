/* Issue 1011 — `NrosCdrBlob` must stay a C++11 AGGREGATE.
 *
 * The Zephyr lane compiles the backend with `-std=c++11`, and a class with
 * default member initialisers is not an aggregate before C++14 — C++14 relaxed
 * exactly that rule. So two NSDMIs that read as harmless defaults turned
 * `NrosCdrBlob{data, len}` in `publisher.cpp` and `service.cpp` into a call to
 * a two-argument constructor that does not exist:
 *
 *     error: no matching function for call to
 *       'nros_rmw_cyclonedds::NrosCdrBlob::NrosCdrBlob(<brace-enclosed initializer list>)'
 *
 * Every other lane builds this TU at C++14 or later and was fine, which is why
 * it was invisible for as long as it was: six zephyr cyclonedds leaves failed
 * to build and nothing else did.
 *
 * This TU is compiled AS C++11 on purpose (see tests/CMakeLists.txt). It is the
 * only place in the tree that pins that standard, so it is the only thing that
 * can catch a re-added initialiser before the Zephyr lane does.
 *
 * The static_asserts guard the other half: `sertype_zero_samples` memsets these
 * and `sertype_realloc_samples` hands them to `dds_realloc`, both of which treat
 * the sample as raw storage. Trivial copyability is what makes that defined;
 * standard layout is what makes the `static_cast` from `void*` defined.
 */

#include <cstddef>
#include <cstdint>
#include <cstring>
#include <type_traits>

#include "nros_sertype.hpp"

using nros_rmw_cyclonedds::NrosCdrBlob;

static_assert(std::is_trivially_copyable<NrosCdrBlob>::value,
              "sertype_zero_samples memsets this and sertype_realloc_samples "
              "dds_reallocs it; both need trivial copyability");
static_assert(std::is_standard_layout<NrosCdrBlob>::value,
              "the sertype callbacks static_cast this from void*");
static_assert(std::is_trivially_default_constructible<NrosCdrBlob>::value,
              "raw storage: a default member initialiser would break this AND "
              "aggregate initialisation under C++11 (issue 1011)");

int main() {
    /* The exact shape publisher.cpp:287 and service.cpp:506 use. Under C++11
     * this only compiles while the type is an aggregate. */
    const uint8_t bytes[4] = {0, 1, 2, 3};
    const NrosCdrBlob blob{bytes, sizeof(bytes)};
    if (blob.data != bytes || blob.size != sizeof(bytes)) {
        return 1;
    }

    /* Value-initialisation still zeroes without the initialisers. */
    NrosCdrBlob zeroed{};
    if (zeroed.data != nullptr || zeroed.size != 0) {
        return 1;
    }

    /* And the raw-storage path the sertype callbacks take. */
    NrosCdrBlob samples[2];
    std::memset(samples, 0, sizeof(NrosCdrBlob) * 2);
    if (samples[1].size != 0) {
        return 1;
    }

    return 0;
}
