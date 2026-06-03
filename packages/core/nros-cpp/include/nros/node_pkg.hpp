// nros-cpp: component-mode declarations
// Freestanding C++ - no exceptions, no STL required

/**
 * @file component.hpp
 * @ingroup grp_node
 * @brief Component registration API for metadata and generated runtimes.
 */

#ifndef NROS_CPP_COMPONENT_HPP
#define NROS_CPP_COMPONENT_HPP

#include "nros/declared_node.hpp"

namespace nros {

// Phase 212.M.5.a.1 — the hardcoded `__nros_component_register` constant
// is retired. Each Component pkg now exports a per-pkg mangled
// `__nros_component_<sanitised_pkg>_register` symbol; codegen derives
// the name from workspace metadata's pkg identity.
constexpr const char* MISSING_COMPONENT_EXPORT_ERROR = "package has no exported nros component";

struct ComponentContextOps {
    using CreateNodeFn = int32_t (*)(void* user_data, const char* stable_id,
                                     const NodeOptions* options, ComponentNode* out_node);
    using CreateEntityFn = int32_t (*)(void* user_data,
                                       const ComponentEntityDescriptor* descriptor);
    using RecordCallbackEffectFn = int32_t (*)(void* user_data, const char* callback_id,
                                               CallbackEffectKind kind, const char* entity_id);

    CreateNodeFn create_node;
    CreateEntityFn create_entity;
    RecordCallbackEffectFn record_callback_effect;
};

class ComponentContext {
  public:
    ComponentContext(void* user_data, const ComponentContextOps* ops)
        : user_data_(user_data), ops_(ops) {}

    Result create_node(ComponentNode& out, const char* stable_id, const NodeOptions& options) {
        if (!ops_ || !ops_->create_node || !stable_id) return Result(ErrorCode::InvalidArgument);
        return Result(ops_->create_node(user_data_, stable_id, &options, &out));
    }

    Result create_entity(const ComponentEntityDescriptor& descriptor) {
        if (!ops_ || !ops_->create_entity) return Result(ErrorCode::InvalidArgument);
        return Result(ops_->create_entity(user_data_, &descriptor));
    }

    Result record_callback_effect(const char* callback_id, CallbackEffectKind kind,
                                  const char* entity_id) {
        if (!ops_ || !ops_->record_callback_effect || !callback_id || !entity_id) {
            return Result(ErrorCode::InvalidArgument);
        }
        return Result(ops_->record_callback_effect(user_data_, callback_id, kind, entity_id));
    }

  private:
    void* user_data_;
    const ComponentContextOps* ops_;
};

inline Result ComponentNode::create_entity(const ComponentEntityDescriptor& descriptor) {
    if (!context_) return Result(ErrorCode::NotInitialized);
    return context_->create_entity(descriptor);
}

using ComponentRegisterFn = int32_t (*)(ComponentContext& context);

} // namespace nros

// Phase 212.M.5.a.1 — per-pkg mangled register symbol.
//
// `NROS_PKG_NAME` is a bare token (pre-sanitised — `-` → `_`) injected by
// the cmake glue (`nano_ros_component_register()` adds it via
// `target_compile_definitions`). Hand-written C++ pkgs that don't go
// through the cmake fn must `#define NROS_PKG_NAME my_pkg` before
// including this header.
#ifndef NROS_PKG_NAME
#define NROS_PKG_NAME unknown
#endif

#define _NROS_CPP_CONCAT(a, b) a##b
#define _NROS_CPP_CONCAT_X(a, b) _NROS_CPP_CONCAT(a, b)
#define _NROS_CPP_REG_SYM(pkg) _NROS_CPP_CONCAT_X(__nros_component_, _NROS_CPP_CONCAT_X(pkg, _register))
#define _NROS_CPP_PRESENT_SYM(pkg) _NROS_CPP_CONCAT_X(__NROS_NODE_PKG_, _NROS_CPP_CONCAT_X(pkg, _EXPORT_PRESENT))
#define _NROS_CPP_CLASS_SYM(pkg) _NROS_CPP_CONCAT_X(__nros_component_, _NROS_CPP_CONCAT_X(pkg, _class_name))

#define NROS_COMPONENTS_REGISTER_NODE(ComponentType)                                               \
    extern "C" int32_t _NROS_CPP_REG_SYM(NROS_PKG_NAME)(::nros::ComponentContext& context) {       \
        return (ComponentType::register_component(context)).raw();                                 \
    }                                                                                              \
    extern "C" const unsigned char _NROS_CPP_PRESENT_SYM(NROS_PKG_NAME) = 1

// Phase 212.L.9 — C++ counterpart of Rust's `nros::component!()` macro.
//
// Emits the C-ABI register trampoline + a stable export marker so the
// generated `system_main` (from `nros codegen-system`) can resolve and
// invoke the user's component class. `QualifiedClassName` is a string
// literal of shape `"<pkg>::<UserClass>"` and the cmake fn
// `nano_ros_component_register()` enforces the prefix match (L.4).
//
// Phase 212.M.5.a.1 — the register symbol is per-pkg mangled via
// `NROS_PKG_NAME` (cmake glue injects it). Equivalent to
// `NROS_COMPONENTS_REGISTER_NODE(UserClass)` but adds a fixed-storage
// symbol carrying the qualified class string so the codegen + lint side
// can sanity-check the binding.
#define NROS_NODE_REGISTER(UserClass, QualifiedClassName)                                     \
    extern "C" int32_t _NROS_CPP_REG_SYM(NROS_PKG_NAME)(::nros::ComponentContext& context) {       \
        return (UserClass::register_component(context)).raw();                                     \
    }                                                                                              \
    extern "C" const unsigned char _NROS_CPP_PRESENT_SYM(NROS_PKG_NAME) = 1;                       \
    extern "C" const char _NROS_CPP_CLASS_SYM(NROS_PKG_NAME)[] = QualifiedClassName

#endif // NROS_CPP_COMPONENT_HPP
