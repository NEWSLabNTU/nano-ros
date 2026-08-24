#!/usr/bin/env python3
"""Which ROS 2 declarations are PUBLIC API, and which are the library's insides.

Phase 379. nano-ros aligns to the API a ROS 2 user writes. It does not align to
rclcpp's callback type-erasure, rcl's wait-set plumbing, or the generated
accessors of `rcl_interfaces`, and counting those as gaps manufactures work that
should never be done.

# The rule is the DECLARING FILE, not a name

Every record carries the file it was declared in -- a header for C and C++
(clang), a source path for Rust (rustdoc spans). A path is a fact about the
library's own organisation; a name is a guess about intent. `AnyExecutable`
looks internal and is; `Waitable` does not and is. Their headers say so either
way.

Three tiers, in order of how much judgement each needs:

1. **Message packages.** `*_msgs`, `*_interfaces`, `*_srvs`, `rosidl_*`,
   `builtin_interfaces`. Their contents are CODEGEN OUTPUT on both sides,
   governed by RFC-0023/0033, and comparing them here compares two code
   generators rather than two APIs. This alone is ~150 of the C lane's 632
   `theirs-only` rows -- every `rcl_interfaces__srv__GetParameters_Request__init`
   and friend.
2. **`detail/` directories.** Upstream's own universal marker for "not API",
   used by rcl, rclcpp and every rosidl-generated package.
3. **Named plumbing.** A short list, each entry with the reason it is not
   something a user writes. This is the only tier that is judgement, which is
   why it is enumerated here rather than expressed as a pattern.

`--include-internal` turns all of this off. The report always says how many
records each tier removed, because a filter that quietly shrinks a number is
indistinguishable from progress.
"""

import os
import re

# Tier 1 -- codegen output, compared by RFC-0023/0033 and not by this tool.
MESSAGE_PACKAGE = re.compile(
    r"(^|/)(rosidl_\w+|builtin_interfaces|unique_identifier_msgs"
    r"|[a-z0-9_]+_(msgs|srvs|interfaces|actions))(/|$)"
)

# Tier 2 -- upstream's own marker.
DETAIL_DIR = re.compile(r"(^|/)detail(/|$)")

# Tier 3 -- named plumbing. Key is a path fragment matched anywhere in the
# declaring file; value is why a user never writes it.
#
# Kept deliberately short. Anything defensible only by "it looks internal"
# belongs in the report as a `declined` ledger row, where somebody has to say so
# in a sentence, not here where it disappears.
PLUMBING = {
    # --- rclcpp: callback type erasure and executor dispatch ---
    "rclcpp/any_executable.h": "the wait-set result rclcpp executors pass between themselves; a user names neither the type nor its fields",
    "rclcpp/any_subscription_callback.h": "type erasure over the callback signatures a user writes; the user writes the lambda, never this",
    "rclcpp/any_service_callback.h": "same type erasure for services",
    "rclcpp/graph_listener.h": "the background thread rclcpp runs to service graph events; reached only through Node::get_graph_event",
    "rclcpp/memory_strategy.h": "allocator policy plumbing for rclcpp's executors; listed under upstream's own \"internal API's and utilities\"",
    "rclcpp/memory_strategies.h": "as memory_strategy.hpp",
    "rclcpp/message_memory_strategy.h": "as memory_strategy.hpp",
    "rclcpp/strategies/": "the memory-strategy implementations",
    "rclcpp/allocator/": "allocator adapters; upstream lists them as internal",
    "rclcpp/contexts/": "the default-context singleton's implementation",
    "rclcpp/function_traits.h": "compile-time introspection rclcpp uses to accept callback shapes",
    "rclcpp/is_ros_compatible_type.h": "a type trait used by the create_* templates",
    "rclcpp/get_message_type_support_handle.h": "typesupport lookup performed by the create_* templates",
    "rclcpp/type_support_decl.h": "typesupport declarations, not user API",
    "rclcpp/typesupport_helpers.h": "runtime typesupport loading, used by generic pub/sub",
    "rclcpp/expand_topic_or_service_name.h": "name expansion rclcpp performs inside create_*",
    "rclcpp/visibility_control.h": "dllexport macros",
    "rclcpp/macros.h": "SharedPtr/UniquePtr boilerplate macros",
    "rclcpp/intra_process_buffer_type.h": "intra-process transport configuration; nano-ros has no intra-process path (RFC-0002)",
    "rclcpp/intra_process_setting.h": "as intra_process_buffer_type.hpp",
    "rclcpp/experimental/": "explicitly unstable upstream",
    "rclcpp/node_interfaces/": "the interface split rclcpp uses so components can take a slice of a Node; a user calls Node's methods",
    # --- rcl / rclc ---
    "rclc/executor_handle.h": "rclc's internal per-entity executor bookkeeping",
    "rcl/arguments.h": "command-line argument parsing; embedded images have no argv (RFC-0045 bakes boot config instead)",
    # --- rclrs ---
    "rclrs/src/subscription/any_subscription_callback.rs": "type erasure over callback signatures, as rclcpp's",
    "rclrs/src/service/any_service_callback.rs": "the service half of the same erasure",
    "rclrs/src/client/any_client_output_sender.rs": "the client half of the same erasure",
    "rclrs/src/rcl_bindings.rs": "raw bindgen output for rcl; the C ABI, not the Rust API",
}


def classify(path):
    """(is_public, tier, reason) for a declaring file path."""
    if not path:
        # No attribution means the extractor could not say where this came
        # from. Treat it as public: dropping an item on missing evidence hides
        # a real gap, while keeping it costs one row somebody can classify.
        return True, None, ""
    norm = path.replace(os.sep, "/")
    if MESSAGE_PACKAGE.search(norm):
        return False, "message", "generated message package; compared by RFC-0023/0033, not here"
    if DETAIL_DIR.search(norm):
        return False, "detail", "in a detail/ directory, upstream's own marker for not-API"
    for fragment, why in PLUMBING.items():
        if fragment in norm:
            return False, "plumbing", why
    return True, None, ""


def filter_records(records):
    """(public_records, {tier: count}) -- what survives, and what each tier took."""
    kept = []
    removed = {}
    for rec in records:
        ok, tier, _ = classify(rec.get("header", ""))
        if ok:
            kept.append(rec)
        else:
            removed[tier] = removed.get(tier, 0) + 1
    return kept, removed


def self_test():
    failures = []

    def check(path, want_public, label):
        got = classify(path)[0]
        if got != want_public:
            failures.append("%s: %r public=%s want %s" % (label, path, got, want_public))

    check(
        "/opt/ros/humble/include/rcl_interfaces/rcl_interfaces/srv/detail/"
        "get_parameters__functions.h",
        False,
        "generated message accessors",
    )
    check("/opt/ros/humble/include/rclcpp/rclcpp/node.hpp", True, "Node is API")
    check("/opt/ros/humble/include/rclcpp/rclcpp/qos.hpp", True, "QoS is API")
    check("/opt/ros/humble/include/rclcpp/rclcpp/clock.hpp", True, "Clock is API")
    check("/opt/ros/humble/include/rclcpp/rclcpp/duration.hpp", True, "Duration is API")
    check(
        "/opt/ros/humble/include/rclcpp/rclcpp/any_executable.hpp",
        False,
        "AnyExecutable is plumbing",
    )
    check(
        "/opt/ros/humble/include/rclcpp/rclcpp/node_interfaces/node_base.hpp",
        False,
        "node_interfaces is plumbing",
    )
    check("/opt/ros/humble/include/rcl/rcl/publisher.h", True, "rcl publisher is API")
    check("rclrs/src/node.rs", True, "rclrs Node is API")
    check(
        "rclrs/src/subscription/any_subscription_callback.rs",
        False,
        "rclrs callback erasure is plumbing",
    )
    check("", True, "no attribution keeps the row")

    # A package whose name merely CONTAINS a message suffix is not a message
    # package: `rclcpp_lifecycle` must survive, `lifecycle_msgs` must not.
    check(
        "/opt/ros/humble/include/rclcpp_lifecycle/rclcpp_lifecycle/lifecycle_node.hpp",
        True,
        "rclcpp_lifecycle is API",
    )
    check(
        "/opt/ros/humble/include/lifecycle_msgs/lifecycle_msgs/msg/state.hpp",
        False,
        "lifecycle_msgs is a message package",
    )

    for why in PLUMBING.values():
        if not why.strip():
            failures.append("a PLUMBING entry has no reason")

    return failures
