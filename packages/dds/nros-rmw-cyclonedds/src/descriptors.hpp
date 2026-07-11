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

/**
 * Resolve the effective DDS message type for a topic publisher /
 * subscriber. The action layer (`executor/action.rs`) creates the
 * feedback topic (`<action>/_action/feedback`) carrying the bare action
 * base type `<pkg>::action::dds_::<A>_`, but the registered descriptor
 * is the synthesised `<A>_FeedbackMessage_`. Derive it from the topic
 * suffix. All non-action topics pass @p type_name through unchanged.
 * Writes a NUL-terminated string into @p out; returns false on overflow.
 */
bool action_topic_type(const char *topic_name, const char *type_name,
                       char *out, std::size_t out_cap);

/**
 * Convert a ROS-form type name (`pkg/msg/Name`) to the DDS-mangled registry
 * key (`pkg::msg::dds_::Name_`). Slash-less input (already DDS-form, or any
 * name without `/`) passes through untouched. Defined in `service.cpp`.
 */
bool ros_form_to_dds(const char *type_name, char *out, std::size_t out_cap);

} // namespace nros_rmw_cyclonedds

#endif // NROS_RMW_CYCLONEDDS_DESCRIPTORS_HPP
