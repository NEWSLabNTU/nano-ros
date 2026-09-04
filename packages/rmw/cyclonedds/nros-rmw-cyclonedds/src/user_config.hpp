// phase-206 W2 — the user's own Cyclone config, baked into the image.
//
// THE PROBLEM THIS SOLVES. On a hosted ROS 2 system a user configures
// CycloneDDS by setting `CYCLONEDDS_URI` to an XML file naming an IP or a
// device, and the application code knows nothing about it. On an RTOS there is
// no environment to set and no filesystem to point at: `env_lookup` returns
// `nullptr` on a freestanding target (`env_compat.hpp`), so that rung is
// structurally dead and the user has no way in at all.
//
// So the file is BAKED. `nros_bake_rmw_user_config()` (NanoRosRmwUserConfig.cmake)
// reads `<bringup>/rmw/cyclonedds.xml` at configure time and generates
// `nros_cyclonedds_user_config.h` next to it, holding the file's BYTES as a
// raw string literal in `nros_rmw_user_config::kCYCLONEDDS_config`.
//
// VERBATIM IS THE WHOLE POINT. nano-ros does not parse the XML, does not
// validate it, and does not re-spell it in a nano-ros schema — the bytes go to
// `dds_create_domain`, which hands any comma token starting with `<` to
// `ddsrt_xmlp_new_string`, Cyclone's own parser. A user writes the same
// document ROS 2 documents, and their existing knowledge transfers.
//
// A RAW STRING LITERAL, not an escaped one: XML is full of `"` and a
// `configure_file`-escaped blob is unreadable in the generated header and
// unreviewable in a diff. `R"NROSXML(...)NROSXML"` cannot collide with the
// document's own content unless it literally contains `)NROSXML"`, which the
// generator checks for and refuses.
#ifndef NROS_RMW_CYCLONEDDS_USER_CONFIG_HPP
#define NROS_RMW_CYCLONEDDS_USER_CONFIG_HPP

// `__has_include` rather than a cmake-defined flag: the header exists only when
// a bringup ships a config, and an image with no config must compile with no
// cmake participation at all. A backend built standalone (the ctest lane, an
// out-of-tree consumer) has no bringup and needs no knowledge of one.
#if defined(__has_include)
#  if __has_include("nros_cyclonedds_user_config.h")
#    include "nros_cyclonedds_user_config.h"
#  endif
#endif

namespace nros_rmw_cyclonedds {

/// The bringup's `rmw/cyclonedds.xml`, verbatim, or `""` when it ships none.
///
/// Empty is the honest default, and `compose_cyclone_config` skips empty
/// fragments — so "no bringup config" costs one byte of rodata and no branch.
#ifdef NROS_CYCLONEDDS_USER_CONFIG_PRESENT
inline constexpr const char* kUserCycloneConfig =
    nros_rmw_user_config::kCYCLONEDDS_config;
#else
inline constexpr const char* kUserCycloneConfig = "";
#endif

/// Does this image carry a user config? Decides whether the HOSTED path has to
/// create the domain itself — see `session.cpp`.
inline constexpr bool has_user_cyclone_config() {
    return kUserCycloneConfig[0] != '\0';
}

}  // namespace nros_rmw_cyclonedds

#endif  // NROS_RMW_CYCLONEDDS_USER_CONFIG_HPP
