// nros-cpp: component-mode declarations
// Freestanding C++ - no exceptions, no STL required

/**
 * @file component.hpp
 * @ingroup grp_node
 * @brief Component registration API for metadata and generated runtimes.
 */

#ifndef NROS_CPP_COMPONENT_HPP
#define NROS_CPP_COMPONENT_HPP

#include "nros/component_node.hpp"

namespace nros {

constexpr const char* COMPONENT_EXPORT_SYMBOL = "__nros_component_register";
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

#define NROS_COMPONENTS_REGISTER_NODE(ComponentType)                                               \
    extern "C" int32_t __nros_component_register(::nros::ComponentContext& context) {              \
        return (ComponentType::register_component(context)).raw();                                 \
    }                                                                                              \
    extern "C" const unsigned char __NROS_COMPONENT_EXPORT_PRESENT = 1

#endif // NROS_CPP_COMPONENT_HPP
