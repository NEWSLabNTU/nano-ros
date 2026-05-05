#ifndef NROS_RMW_CYCLONEDDS_DESCRIPTORS_HPP
#define NROS_RMW_CYCLONEDDS_DESCRIPTORS_HPP

#include <cstddef>

#include <dds/dds.h>

namespace nros_rmw_cyclonedds {

/**
 * Register a Cyclone topic descriptor under @p type_name.
 *
 * Called from auto-generated `<idl_stem>_register.c` translation
 * units at static-init time. Idempotent — re-registration under the
 * same name is silently ignored.
 */
void register_descriptor(const char *type_name,
                         const dds_topic_descriptor_t *descriptor);

/**
 * Find a previously registered descriptor by @p type_name, or
 * `nullptr` if none.
 */
const dds_topic_descriptor_t *find_descriptor(const char *type_name);

/** Number of registered descriptors. Useful for diagnostics + tests. */
std::size_t registered_descriptor_count();

} // namespace nros_rmw_cyclonedds

#endif // NROS_RMW_CYCLONEDDS_DESCRIPTORS_HPP
