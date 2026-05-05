#ifndef NROS_RMW_CYCLONEDDS_TOPIC_PREFIX_HPP
#define NROS_RMW_CYCLONEDDS_TOPIC_PREFIX_HPP

// Phase 117.X.2 — ROS 2 topic-name prefix conventions.
//
// `rmw_cyclonedds_cpp` tags every DDS topic name with a 3-letter
// prefix indicating the kind of traffic it carries:
//
//   rt/<topic>            user pub/sub
//   rq/<svc>Request       service Request topic
//   rr/<svc>Reply         service Reply topic
//
// (Action prefixes `rs/` / `rq/` / `rr/` follow the same scheme but
// service is what we use today.)
//
// Without these prefixes, DDS topic-name matching fails: a stock
// `rclcpp` subscriber on `chatter` listens to `rt/chatter` on the
// wire and never matches our publisher on raw `chatter`.
//
// We apply the prefix inside the backend so the runtime API stays
// ROS-shaped (consumer passes `chatter`, gets ROS-2-interop
// behaviour without configuring anything).
//
// **Idempotent.** If the runtime supplies a name that already
// starts with `rt/`, `rq/`, `rr/`, or `rs/` we don't double-prefix.
// **Opt-out.** Setting `NROS_RMW_CYCLONEDDS_SKIP_PREFIX=1` in the
// environment disables prefixing entirely — for raw-DDS deployments
// that already manage their own naming (e.g. autoware-safety-island
// uses unprefixed custom IDLs).

#include <cstddef>
#include <cstdlib>
#include <cstring>

namespace nros_rmw_cyclonedds {

namespace topic_prefix {

inline bool skip_via_env() {
    const char *env = std::getenv("NROS_RMW_CYCLONEDDS_SKIP_PREFIX");
    return env != nullptr && env[0] != '\0' && env[0] != '0';
}

// True when @p name already starts with one of the 3-char ROS 2
// prefixes followed by `/`. Idempotent guard so callers that
// already deliver fully-qualified names (e.g. tests, raw-DDS apps)
// don't get double-prefixed.
inline bool already_prefixed(const char *name) {
    if (name == nullptr || name[0] == '\0') return false;
    if (name[0] != 'r') return false;
    char c = name[1];
    if (c != 't' && c != 'q' && c != 'r' && c != 's') return false;
    return name[2] == '/';
}

// Apply @p prefix (e.g. "rt") + '/' + @p name into @p out (size
// @p out_cap). Returns false on overflow. When the env opt-out is
// set, or the name is already prefixed, copies @p name verbatim.
inline bool apply(const char *name, const char *prefix, char *out,
                  std::size_t out_cap) {
    if (name == nullptr || prefix == nullptr || out == nullptr) return false;
    std::size_t name_len = std::strlen(name);

    if (skip_via_env() || already_prefixed(name)) {
        if (name_len + 1 > out_cap) return false;
        std::memcpy(out, name, name_len + 1);
        return true;
    }

    std::size_t prefix_len = std::strlen(prefix);
    if (prefix_len + 1 + name_len + 1 > out_cap) return false;
    std::memcpy(out, prefix, prefix_len);
    out[prefix_len] = '/';
    std::memcpy(out + prefix_len + 1, name, name_len + 1);
    return true;
}

} // namespace topic_prefix

} // namespace nros_rmw_cyclonedds

#endif // NROS_RMW_CYCLONEDDS_TOPIC_PREFIX_HPP
