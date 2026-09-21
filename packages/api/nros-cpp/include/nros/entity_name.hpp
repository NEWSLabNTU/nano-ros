// nros-cpp: the ONE copy for a fixed-capacity entity-name field.
// Freestanding C++ -- no exceptions, no STL required.

/**
 * @file entity_name.hpp
 * @ingroup grp_support
 * @brief `nros::detail::assign_entity_name` and the service-name bound.
 */

#ifndef NROS_CPP_ENTITY_NAME_HPP
#define NROS_CPP_ENTITY_NAME_HPP

#include <cstddef>

namespace nros {

/// Bound on the service name a `Client` / `Service` remembers for
/// `get_service_name()` — phase-444.
///
/// ONE spelling for both, matching `nros::PUBLISHER_TOPIC_NAME_MAX`,
/// `nros::SUBSCRIPTION_TOPIC_NAME_MAX`, `nros::ACTION_NAME_MAX` and the C
/// surface's `NROS_MAX_SERVICE_NAME_LEN`, so a name that fits one entity fits
/// all of them and a truncation is not a per-class surprise.
static constexpr size_t SERVICE_NAME_MAX = 256;

namespace detail {

/// Copy `src` into a fixed `char[N]` entity-name field, NUL-terminated.
/// Truncation is silent; a null `src` yields the empty string.
///
/// **ONE spelling, phase-444.** Eight entity classes keep the name they were
/// created on in a `char[256]` member — `Publisher`, `Subscription`, the two
/// action tiers' client and server, and now `Client` and `Service` — and each
/// had hand-written the same five-line loop. CLAUDE.md's rule after issue #326
/// is to add a shared helper rather than a seventh and eighth copy of an
/// idiom, so this is that helper and every site calls it.
///
/// `N` is deduced from the array, so the bound can never be passed wrong; that
/// is the defect a `memcpy` with a hand-written length has and this does not.
template <size_t N> inline void assign_entity_name(char (&dst)[N], const char* src) {
    size_t i = 0;
    if (src != nullptr) {
        for (; i + 1 < N && src[i] != '\0'; ++i) {
            dst[i] = src[i];
        }
    }
    dst[i] = '\0';
}

} // namespace detail
} // namespace nros

#endif // NROS_CPP_ENTITY_NAME_HPP
