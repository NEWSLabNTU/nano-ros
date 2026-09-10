// phase-446 W6 -- the DECLARED parameters reach `Node`, and the
// lookups it makes at boot answer what the contract says.
//
// The check itself runs at BOOT, not at compile time: its key is the node's
// fully-qualified name, a constructor argument. But every lookup it makes is a
// `constexpr` function over a `constexpr` table, so this TU can assert each
// answer the boot check depends on -- a table that stopped matching node names
// (and so checked nothing) fails here, not on a board.
//
// The table comes from `declared-params-fixture/nros/`, which is what
// `nros ws entity-inventory --output-params-header` renders from the
// `declared_params.yaml` beside it. In a real build `nano_ros_node_register()`
// writes that header into the component library's own include dir; here the
// gate puts the fixture dir on the include path, which exercises the same
// `__has_include` pickup.
//
// `just check cpp` compiles this with `-fsyntax-only -std=c++17`.
#include <nros/nros.hpp>

namespace nros_cpp_declared_params_compile_test {

constexpr const ::nros::declared_params::Node* MRM =
    ::nros::declared_param_node("/system", "mrm_handler");

// The table was found at all -- without this every assertion below could hold
// against an empty table.
static_assert(::nros::declared_params::NODE_COUNT == 1, "the fixture declares one node");
static_assert(::nros::declared_params::ROW_COUNT == 4, "the fixture declares four parameters");

// The node is found by namespace + name, whatever the namespace's spelling.
static_assert(MRM != nullptr, "a declaring node is found by its FQN");
static_assert(::nros::declared_param_node("/system/", "mrm_handler") == MRM,
              "a trailing `/` on the namespace is the same namespace");
// Another namespace is another node, and a node with no `params:` is absent --
// so neither is checked.
static_assert(::nros::declared_param_node("/other", "mrm_handler") == nullptr,
              "the same name in another namespace is another node");
static_assert(::nros::declared_param_node("/", "talker") == nullptr,
              "a node whose contract has no `params:` is not in the table");
static_assert(::nros::declared_param_type(nullptr, "anything") ==
                  ::nros::DECLARED_PARAM_NODE_UNCHECKED,
              "an absent node is UNCHECKED, not undeclared");

// Each declared name answers its declared type.
static_assert(::nros::declared_param_type(MRM, "update_rate") == ::nros::param_type::INTEGER, "");
static_assert(::nros::declared_param_type(MRM, "timeout_operation_mode_availability") ==
                  ::nros::param_type::DOUBLE,
              "");
static_assert(::nros::declared_param_type(MRM, "use_emergency_holding") == ::nros::param_type::BOOL,
              "");
static_assert(::nros::declared_param_type(MRM, "gains") == ::nros::param_type::DOUBLE_ARRAY, "");
// ...and an undeclared name answers UNDECLARED, which is what refuses the boot.
static_assert(::nros::declared_param_type(MRM, "timeout") == ::nros::DECLARED_PARAM_UNDECLARED, "");

// The names every node carries, exactly as play_launch exempts them.
static_assert(::nros::declared_param_exempt("use_sim_time"), "");
static_assert(::nros::declared_param_exempt("start_type_description_service"), "");
static_assert(::nros::declared_param_exempt("qos_overrides./chatter.subscription.depth"), "");
static_assert(!::nros::declared_param_exempt("update_rate"), "");

// The type `declare_parameter<T>` declares, for each store overload.
static_assert(::nros::detail::node_param_type<bool>::value == ::nros::param_type::BOOL, "");
static_assert(::nros::detail::node_param_type<int>::value == ::nros::param_type::INTEGER, "");
static_assert(::nros::detail::node_param_type<int64_t>::value == ::nros::param_type::INTEGER, "");
static_assert(::nros::detail::node_param_type<double>::value == ::nros::param_type::DOUBLE, "");
static_assert(::nros::detail::node_param_type<const char*>::value == ::nros::param_type::STRING,
              "");

} // namespace nros_cpp_declared_params_compile_test
