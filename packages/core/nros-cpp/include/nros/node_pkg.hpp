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

static constexpr size_t DECLARED_NODE_SYNTHETIC_ID_MAX = 96;

namespace detail {

inline bool is_declared_id_alnum(char c) {
    return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9');
}

inline char declared_id_lower(char c) {
    return (c >= 'A' && c <= 'Z') ? static_cast<char>(c - 'A' + 'a') : c;
}

template <size_t N>
inline Result synthesize_declared_entity_id(char (&out)[N], const char* prefix,
                                            const char* source_name) {
    if (!prefix || !source_name || N == 0) return Result(ErrorCode::InvalidArgument);

    size_t pos = 0;
    while (*prefix) {
        if (pos + 1 >= N) return Result(ErrorCode::Full);
        out[pos++] = *prefix++;
    }
    if (pos + 1 >= N) return Result(ErrorCode::Full);
    out[pos++] = '_';

    bool emitted_source = false;
    bool last_was_sep = true;
    for (const char* p = source_name; *p; ++p) {
        const char c = *p;
        if (is_declared_id_alnum(c)) {
            if (pos + 1 >= N) return Result(ErrorCode::Full);
            out[pos++] = declared_id_lower(c);
            emitted_source = true;
            last_was_sep = false;
        } else if (emitted_source && !last_was_sep) {
            if (pos + 1 >= N) return Result(ErrorCode::Full);
            out[pos++] = '_';
            last_was_sep = true;
        }
    }
    if (!emitted_source) return Result(ErrorCode::InvalidArgument);
    if (last_was_sep && pos > 0) --pos;
    out[pos] = '\0';
    return Result::success();
}

} // namespace detail

// Phase 212.M.5.a.1 — the hardcoded `__nros_component_register` constant
// is retired. Each Component pkg now exports a per-pkg mangled
// `__nros_component_<sanitised_pkg>_register` symbol; codegen derives
// the name from workspace metadata's pkg identity.
constexpr const char* MISSING_NODE_EXPORT_ERROR = "package has no exported nros component";

struct NodeContextOps {
    using CreateNodeFn = int32_t (*)(void* user_data, const char* stable_id,
                                     const NodeOptions* options, DeclaredNode* out_node);
    using CreateEntityFn = int32_t (*)(void* user_data, const NodeEntityDescriptor* descriptor);
    using RecordCallbackEffectFn = int32_t (*)(void* user_data, const char* callback_id,
                                               CallbackEffectKind kind, const char* entity_id);

    CreateNodeFn create_node;
    CreateEntityFn create_entity;
    RecordCallbackEffectFn record_callback_effect;
};

class NodeContext {
  public:
    NodeContext(void* user_data, const NodeContextOps* ops) : user_data_(user_data), ops_(ops) {}

    Result create_node(DeclaredNode& out, const char* stable_id, const NodeOptions& options) {
        if (!ops_ || !ops_->create_node || !stable_id) return Result(ErrorCode::InvalidArgument);
        out = DeclaredNode(this, stable_id, nullptr);
        Result result(ops_->create_node(user_data_, stable_id, &options, &out));
        if (result.ok()) {
            out = DeclaredNode(this, stable_id, out.runtime_handle());
        } else {
            out = DeclaredNode();
        }
        return result;
    }

    Result create_node(DeclaredNode& out, const NodeOptions& options) {
        if (!options.name) return Result(ErrorCode::InvalidArgument);
        return create_node(out, options.name, options);
    }

    Result create_entity(const NodeEntityDescriptor& descriptor) {
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
    const NodeContextOps* ops_;
};

inline Result DeclaredNode::create_entity(const NodeEntityDescriptor& descriptor) {
    if (!context_) return Result(ErrorCode::NotInitialized);
    return context_->create_entity(descriptor);
}

inline Result DeclaredNode::create_entity(const char* stable_id, NodeEntityKind kind,
                                          const char* source_name, const char* type_name,
                                          const char* type_hash, const char* callback_id) {
    if (!is_valid() || !stable_id || !source_name) return Result(ErrorCode::InvalidArgument);
    NodeEntityDescriptor descriptor{
        /*stable_id*/   stable_id,
        /*node_id*/     stable_id_,
        /*kind*/        kind,
        /*source_name*/ source_name,
        /*type_name*/   type_name ? type_name : "",
        /*type_hash*/   type_hash ? type_hash : "",
        /*callback_id*/ callback_id,
    };
    return create_entity(descriptor);
}

inline Result DeclaredNode::create_publisher(const char* stable_id, const char* topic_name,
                                             const char* type_name, const char* type_hash) {
    return create_entity(stable_id, NodeEntityKind::Publisher, topic_name, type_name, type_hash,
                         nullptr);
}

inline Result DeclaredNode::create_subscription(const char* stable_id, const char* topic_name,
                                                const char* type_name, const char* callback_id,
                                                const char* type_hash) {
    return create_entity(stable_id, NodeEntityKind::Subscription, topic_name, type_name, type_hash,
                         callback_id);
}

inline Result DeclaredNode::create_timer(const char* stable_id, const char* period_ms,
                                         const char* callback_id) {
    return create_entity(stable_id, NodeEntityKind::Timer, period_ms, "", "", callback_id);
}

template <typename M>
inline Result DeclaredNode::create_publisher(const char* topic_name, const QoS& qos) {
    (void)qos;
    char stable_id[DECLARED_NODE_SYNTHETIC_ID_MAX];
    Result result = detail::synthesize_declared_entity_id(stable_id, "pub", topic_name);
    if (!result.ok()) return result;
    return create_publisher(stable_id, topic_name, M::TYPE_NAME, M::TYPE_HASH);
}

template <typename M>
inline Result DeclaredNode::create_subscription(const char* topic_name, const char* callback_id,
                                                const QoS& qos) {
    (void)qos;
    char stable_id[DECLARED_NODE_SYNTHETIC_ID_MAX];
    Result result = detail::synthesize_declared_entity_id(stable_id, "sub", topic_name);
    if (!result.ok()) return result;
    return create_subscription(stable_id, topic_name, M::TYPE_NAME, callback_id, M::TYPE_HASH);
}

using NodeRegisterFn = int32_t (*)(NodeContext& context);

} // namespace nros

// Phase 212.M.5.a.1 — per-pkg mangled register symbol.
//
// `NROS_PKG_NAME` is a bare token (pre-sanitised — `-` → `_`) injected by
// the cmake glue (`nano_ros_node_register()` adds it via
// `target_compile_definitions`). Hand-written C++ pkgs that don't go
// through the cmake fn must `#define NROS_PKG_NAME my_pkg` before
// including this header.
#ifndef NROS_PKG_NAME
#define NROS_PKG_NAME unknown
#endif

#define _NROS_CPP_CONCAT(a, b) a##b
#define _NROS_CPP_CONCAT_X(a, b) _NROS_CPP_CONCAT(a, b)
#define _NROS_CPP_REG_SYM(pkg)                                                                     \
    _NROS_CPP_CONCAT_X(__nros_component_, _NROS_CPP_CONCAT_X(pkg, _register))
#define _NROS_CPP_PRESENT_SYM(pkg)                                                                 \
    _NROS_CPP_CONCAT_X(__NROS_NODE_PKG_, _NROS_CPP_CONCAT_X(pkg, _EXPORT_PRESENT))
#define _NROS_CPP_CLASS_SYM(pkg)                                                                   \
    _NROS_CPP_CONCAT_X(__nros_component_, _NROS_CPP_CONCAT_X(pkg, _class_name))

#define NROS_NODE_PKG_REGISTER(ComponentType)                                                      \
    extern "C" int32_t _NROS_CPP_REG_SYM(NROS_PKG_NAME)(::nros::NodeContext & context) {           \
        return (ComponentType::register_node(context)).raw();                                      \
    }                                                                                              \
    extern "C" const unsigned char _NROS_CPP_PRESENT_SYM(NROS_PKG_NAME) = 1

// Phase 212.L.9 — C++ counterpart of Rust's `nros::component!()` macro.
//
// Emits the C-ABI register trampoline + a stable export marker so the
// generated `system_main` (from `nros codegen-system`) can resolve and
// invoke the user's component class. `QualifiedClassName` is a string
// literal of shape `"<pkg>::<UserClass>"` and the cmake fn
// `nano_ros_node_register()` enforces the prefix match (L.4).
//
// Phase 212.M.5.a.1 — the register symbol is per-pkg mangled via
// `NROS_PKG_NAME` (cmake glue injects it). Equivalent to
// `NROS_NODE_PKG_REGISTER(UserClass)` but adds a fixed-storage
// symbol carrying the qualified class string so the codegen + lint side
// can sanity-check the binding.
#define NROS_NODE_REGISTER(UserClass, QualifiedClassName)                                          \
    extern "C" int32_t _NROS_CPP_REG_SYM(NROS_PKG_NAME)(::nros::NodeContext & context) {           \
        return (UserClass::register_node(context)).raw();                                          \
    }                                                                                              \
    extern "C" const unsigned char _NROS_CPP_PRESENT_SYM(NROS_PKG_NAME) = 1;                       \
    extern "C" const char _NROS_CPP_CLASS_SYM(NROS_PKG_NAME)[] = QualifiedClassName

// Phase 219.H.1 — 1-arg shorthand mirroring Rust's `nros::node!(Talker);`
// ergonomics. Derives the qualified-class-name string at preprocess time
// from `NROS_PKG_NAME` (cmake-injected per 212.M.5.a.1) + the supplied
// `UserClass`, joining them with `::`. Equivalent to writing
// `NROS_NODE_REGISTER(UserClass, "<pkg>::<UserClass>")` by hand.
//
//   namespace talker_pkg {
//       class Talker {
//         public:
//           static ::nros::Result register_node(::nros::NodeContext&);
//       };
//
//       NROS_NODE(Talker);    // inside the namespace; class is unqualified
//   }                          // — same scoping as Rust's `nros::node!()`.
//
// The 2-arg `NROS_NODE_REGISTER` form stays for cases that want an
// explicit override (the class lives in a nested namespace whose path
// does not match `NROS_PKG_NAME::`, the user calls the macro from
// outside the pkg namespace, or for back-compat).
//
// Implementation note — two layers of `#`-stringification are needed
// to expand the cmake-injected `NROS_PKG_NAME` macro before stringifying
// it. `_NROS_STR_INNER` swallows the literal token; `_NROS_STR` forces
// macro expansion first.
#define _NROS_STR_INNER(x) #x
#define _NROS_STR(x) _NROS_STR_INNER(x)
#define NROS_NODE(UserClass)                                                                       \
    NROS_NODE_REGISTER(UserClass, _NROS_STR(NROS_PKG_NAME) "::" _NROS_STR(UserClass))

#endif // NROS_CPP_COMPONENT_HPP
