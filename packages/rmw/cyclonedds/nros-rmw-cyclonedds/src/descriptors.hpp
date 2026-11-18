#ifndef NROS_RMW_CYCLONEDDS_DESCRIPTORS_HPP
#define NROS_RMW_CYCLONEDDS_DESCRIPTORS_HPP

#include <cstddef>

#include <dds/dds.h>

namespace nros_rmw_cyclonedds {

/**
 * The descriptor types the backend registers ON ITS OWN, whatever the image
 * declares: `graph.cpp`'s `ros_discovery_info` type. The table holds these on
 * top of `NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES` (the image's types), and
 * `graph.cpp` looks its type up by this name, so a backend-owned type added
 * here is a row added to the table in the same edit.
 */
constexpr const char *kBackendOwnDescriptorTypes[] = {
    "rmw_dds_common::msg::dds_::ParticipantEntitiesInfo_",
};

/**
 * Register a Cyclone topic descriptor.
 *
 * Called from auto-generated `<idl_stem>_register.c` translation
 * units at static-init time. The descriptor is held under ONE key,
 * descriptor_key() (issue 2704 follow-up). Idempotent -- re-registration
 * under the same key is silently ignored.
 */
void register_descriptor(const char *type_name,
                         const dds_topic_descriptor_t *descriptor);

/** try_register_descriptor() results (issue 2704). */
constexpr int kTryRegisterOk = 0;
constexpr int kTryRegisterInvalid = -1;
constexpr int kTryRegisterFull = -2;

/**
 * The one key a registration of @p descriptor is held under: the
 * descriptor's own `m_typename` (the DDS-mangled `pkg::msg::dds_::Name_`
 * form Cyclone names the topic type by), or @p type_name when the
 * descriptor carries none. NULL when both are NULL.
 *
 * One key per registration, so one table row per registered type: the
 * count every road sizes the table from (`NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES`)
 * is a count of TYPES, and it is the row count with no factor in between.
 * A ROS-form (`pkg/msg/Name`) lookup still resolves: find_descriptor()
 * normalises it with ros_form_to_dds() first.
 */
const char *descriptor_key(const char *type_name, const dds_topic_descriptor_t *descriptor);

/**
 * Register @p descriptor under descriptor_key() (issue 2704).
 *
 * The run-time counterpart of register_descriptor(): where that one can only
 * count a registration the table has no room for (it runs from static
 * constructors), this one REFUSES it and says so. Returns kTryRegisterOk when
 * the descriptor's key is held, including when it already was;
 * kTryRegisterInvalid for a NULL argument; kTryRegisterFull when the key does
 * not fit, in which case nothing is added and nothing is counted as dropped.
 */
int try_register_descriptor(const char *type_name,
                            const dds_topic_descriptor_t *descriptor);

/** The table's capacity in rows: `NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES` plus
 *  kBackendOwnDescriptorTypes. */
std::size_t max_registered_descriptors();

/**
 * Find a previously registered descriptor by @p type_name, or
 * `nullptr` if none. A ROS-form name (`pkg/msg/Name`) is looked up by its
 * DDS form (ros_form_to_dds()), the key it is held under.
 */
const dds_topic_descriptor_t *find_descriptor(const char *type_name);

/** Number of registered descriptors. Useful for diagnostics + tests. */
std::size_t registered_descriptor_count();

/** Number of registrations DROPPED at the static cap (issue 0280). A
 *  non-zero value is reported once to stderr on the first failed lookup. */
std::size_t dropped_descriptor_count();

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
