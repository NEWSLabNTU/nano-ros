/**
 * @file parameter.h
 * @ingroup grp_parameter
 * @brief Parameter server API.
 *
 * Declare, get, and set typed parameters on a local parameter server.
 */

#ifndef NROS_PARAMETER_H
#define NROS_PARAMETER_H

#include "nros/types.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Phase 91.C1: type definitions (nros_parameter_server_state_t,
 * nros_parameter_type_t, nros_parameter_array_t, nros_parameter_value_t,
 * nros_parameter_t, nros_parameter_callback_t, nros_parameter_server_t) come
 * from <nros/nros_generated.h> via the nros/types.h include above.
 *
 * The typed parameter setters / getters (bool / integer / double /
 * *_array) are declared by hand below; the auto-generated header
 * cannot synthesise them from the runtime side, so this file keeps
 * the canonical declarations.
 */

/* ===================================================================
 * Functions
 * =================================================================== */

/**
 * @brief Get a zero-initialized parameter server.
 * @return Zero-initialized `nros_parameter_server_t`.
 */
NROS_PUBLIC struct nros_parameter_server_t nros_parameter_server_get_zero_initialized(void);

/**
 * @brief Initialise a parameter server with user-provided storage.
 *
 * @param server   Pointer to a zero-initialized parameter server.
 * @param storage  Pointer to a user-provided parameter array.
 * @param capacity Maximum number of parameters the array can hold.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_parameter_server_init(struct nros_parameter_server_t* server,
                                      struct nros_parameter_t* storage, size_t capacity);

/**
 * @brief Set a parameter change callback.
 *
 * @param server   Pointer to an initialized parameter server.
 * @param callback Callback function, or NULL to clear.
 * @param context  User context.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_parameter_server_set_callback(struct nros_parameter_server_t* server,
                                              nros_parameter_callback_t callback, void* context);

/**
 * @brief Declare a boolean parameter.
 *
 * @param server        Pointer to an initialized parameter server.
 * @param name          Parameter name (null-terminated).
 * @param default_value Default boolean value.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_parameter_declare_bool(struct nros_parameter_server_t* server, const char* name,
                                       bool default_value);

/**
 * @brief Declare an integer parameter.
 *
 * @param server        Pointer to an initialized parameter server.
 * @param name          Parameter name (null-terminated).
 * @param default_value Default integer value.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_parameter_declare_integer(struct nros_parameter_server_t* server, const char* name,
                                          int64_t default_value);

/**
 * @brief Declare a double parameter.
 *
 * @param server        Pointer to an initialized parameter server.
 * @param name          Parameter name (null-terminated).
 * @param default_value Default double value.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_parameter_declare_double(struct nros_parameter_server_t* server, const char* name,
                                         double default_value);

/**
 * @brief Declare a string parameter.
 *
 * @param server        Pointer to an initialized parameter server.
 * @param name          Parameter name (null-terminated).
 * @param default_value Default string value (null-terminated).
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_parameter_declare_string(struct nros_parameter_server_t* server, const char* name,
                                         const char* default_value);

/**
 * @brief Get a boolean parameter value.
 *
 * @param server Pointer to an initialized parameter server.
 * @param name   Parameter name.
 * @param value  Output: parameter value.
 *
 * @retval NROS_RET_OK        on success.
 * @retval NROS_RET_NOT_FOUND  if the parameter does not exist.
 */
NROS_PUBLIC
nros_ret_t nros_parameter_get_bool(const struct nros_parameter_server_t* server, const char* name,
                                   bool* value);

/**
 * @brief Get an integer parameter value.
 *
 * @param server Pointer to an initialized parameter server.
 * @param name   Parameter name.
 * @param value  Output: parameter value.
 *
 * @retval NROS_RET_OK        on success.
 * @retval NROS_RET_NOT_FOUND  if the parameter does not exist.
 */
NROS_PUBLIC
nros_ret_t nros_parameter_get_integer(const struct nros_parameter_server_t* server,
                                      const char* name, int64_t* value);

/**
 * @brief Get a double parameter value.
 *
 * @param server Pointer to an initialized parameter server.
 * @param name   Parameter name.
 * @param value  Output: parameter value.
 *
 * @retval NROS_RET_OK        on success.
 * @retval NROS_RET_NOT_FOUND  if the parameter does not exist.
 */
NROS_PUBLIC
nros_ret_t nros_parameter_get_double(const struct nros_parameter_server_t* server, const char* name,
                                     double* value);

/**
 * @brief Get a string parameter value.
 *
 * @param server  Pointer to an initialized parameter server.
 * @param name    Parameter name.
 * @param value   Output buffer for the string.
 * @param max_len Maximum length of the output buffer.
 *
 * @retval NROS_RET_OK        on success.
 * @retval NROS_RET_NOT_FOUND  if the parameter does not exist.
 */
NROS_PUBLIC
nros_ret_t nros_parameter_get_string(const struct nros_parameter_server_t* server, const char* name,
                                     char* value, size_t max_len);

/**
 * @brief Set a boolean parameter value.
 *
 * @param server Pointer to an initialized parameter server.
 * @param name   Parameter name.
 * @param value  New boolean value.
 *
 * @retval NROS_RET_OK        on success.
 * @retval NROS_RET_NOT_FOUND  if the parameter does not exist.
 */
NROS_PUBLIC
nros_ret_t nros_parameter_set_bool(struct nros_parameter_server_t* server, const char* name,
                                   bool value);

/**
 * @brief Set an integer parameter value.
 *
 * @param server Pointer to an initialized parameter server.
 * @param name   Parameter name.
 * @param value  New integer value.
 *
 * @retval NROS_RET_OK        on success.
 * @retval NROS_RET_NOT_FOUND  if the parameter does not exist.
 */
NROS_PUBLIC
nros_ret_t nros_parameter_set_integer(struct nros_parameter_server_t* server, const char* name,
                                      int64_t value);

/**
 * @brief Set a double parameter value.
 *
 * @param server Pointer to an initialized parameter server.
 * @param name   Parameter name.
 * @param value  New double value.
 *
 * @retval NROS_RET_OK        on success.
 * @retval NROS_RET_NOT_FOUND  if the parameter does not exist.
 */
NROS_PUBLIC
nros_ret_t nros_parameter_set_double(struct nros_parameter_server_t* server, const char* name,
                                     double value);

/**
 * @brief Set a string parameter value.
 *
 * @param server Pointer to an initialized parameter server.
 * @param name   Parameter name.
 * @param value  New string value (null-terminated).
 *
 * @retval NROS_RET_OK        on success.
 * @retval NROS_RET_NOT_FOUND  if the parameter does not exist.
 */
NROS_PUBLIC
nros_ret_t nros_parameter_set_string(struct nros_parameter_server_t* server, const char* name,
                                     const char* value);

/* -------------------------------------------------------------------
 * Array parameters
 *
 * Array parameters store a pointer + length to caller-owned data.
 * The caller MUST keep the underlying storage alive for the lifetime of
 * the parameter (until @ref nros_parameter_server_fini, or until the
 * parameter is overwritten with a new pointer via the matching `_set`
 * function). String arrays point to an array of `const char*` — each
 * element is itself a null-terminated, caller-owned string.
 * ------------------------------------------------------------------- */

/** @brief Declare a byte array parameter (`uint8_t[]`). */
NROS_PUBLIC
nros_ret_t nros_parameter_declare_byte_array(struct nros_parameter_server_t* server,
                                             const char* name, const uint8_t* data, size_t len);
/** @brief Declare a boolean array parameter (`bool[]`). */
NROS_PUBLIC
nros_ret_t nros_parameter_declare_bool_array(struct nros_parameter_server_t* server,
                                             const char* name, const bool* data, size_t len);
/** @brief Declare an integer array parameter (`int64_t[]`). */
NROS_PUBLIC
nros_ret_t nros_parameter_declare_integer_array(struct nros_parameter_server_t* server,
                                                const char* name, const int64_t* data, size_t len);
/** @brief Declare a double array parameter (`double[]`). */
NROS_PUBLIC
nros_ret_t nros_parameter_declare_double_array(struct nros_parameter_server_t* server,
                                               const char* name, const double* data, size_t len);
/** @brief Declare a string array parameter (array of `const char*`). */
NROS_PUBLIC
nros_ret_t nros_parameter_declare_string_array(struct nros_parameter_server_t* server,
                                               const char* name, const char* const* data,
                                               size_t len);

/**
 * @name Array parameter pointer identity (issue 0340)
 *
 * The array `declare` / `set` calls above take BORROWED storage: the server
 * records the caller's `data` pointer verbatim and never copies the elements,
 * which is why the caller must keep that memory valid for the parameter's
 * lifetime. The `get` calls below return **that same pointer**, unchanged —
 * not a copy, not an interior pointer, not server-owned storage.
 *
 * That identity is LOAD-BEARING, not incidental. `nros/parameter.hpp`'s
 * `ParameterServer` allocates array blocks out of a header-side pool with an
 * out-of-band capacity word written immediately in front of each block, and
 * recovers the capacity on `set` by reading back from the returned pointer.
 * If this side ever starts copying arrays, returning an interior pointer, or
 * handing back storage it owns, that read lands on unrelated memory.
 *
 * The C++ side now verifies the returned pointer lies inside its own pool
 * before trusting it, so a violation surfaces as `NROS_RET_INVALID_ARGUMENT`
 * rather than an out-of-bounds read — but the guarantee is still part of this
 * ABI. Changing it requires changing `parameter.hpp` in the same commit.
 *
 * The proper fix is to carry the capacity in server-owned state (a field on
 * `nros_parameter_array_t`, or a `nros_parameter_get_*_array_capacity()` accessor).
 * That is a breaking change to a hand-mirrored FFI struct and a public C ABI,
 * so it is deferred rather than done silently.
 * @{
 */

/** @brief Get a byte array parameter (returns stored pointer + length). */
NROS_PUBLIC
nros_ret_t nros_parameter_get_byte_array(const struct nros_parameter_server_t* server,
                                         const char* name, const uint8_t** data, size_t* len);
/** @brief Get a boolean array parameter (returns stored pointer + length). */
NROS_PUBLIC
nros_ret_t nros_parameter_get_bool_array(const struct nros_parameter_server_t* server,
                                         const char* name, const bool** data, size_t* len);
/** @brief Get an integer array parameter (returns stored pointer + length). */
NROS_PUBLIC
nros_ret_t nros_parameter_get_integer_array(const struct nros_parameter_server_t* server,
                                            const char* name, const int64_t** data, size_t* len);
/** @brief Get a double array parameter (returns stored pointer + length). */
NROS_PUBLIC
nros_ret_t nros_parameter_get_double_array(const struct nros_parameter_server_t* server,
                                           const char* name, const double** data, size_t* len);
/** @brief Get a string array parameter (returns stored pointer + length). */
NROS_PUBLIC
nros_ret_t nros_parameter_get_string_array(const struct nros_parameter_server_t* server,
                                           const char* name, const char* const** data, size_t* len);
/** @} */

/** @brief Set a byte array parameter (replaces stored pointer + length). */
NROS_PUBLIC
nros_ret_t nros_parameter_set_byte_array(struct nros_parameter_server_t* server, const char* name,
                                         const uint8_t* data, size_t len);
/** @brief Set a boolean array parameter (replaces stored pointer + length). */
NROS_PUBLIC
nros_ret_t nros_parameter_set_bool_array(struct nros_parameter_server_t* server, const char* name,
                                         const bool* data, size_t len);
/** @brief Set an integer array parameter (replaces stored pointer + length). */
NROS_PUBLIC
nros_ret_t nros_parameter_set_integer_array(struct nros_parameter_server_t* server,
                                            const char* name, const int64_t* data, size_t len);
/** @brief Set a double array parameter (replaces stored pointer + length). */
NROS_PUBLIC
nros_ret_t nros_parameter_set_double_array(struct nros_parameter_server_t* server, const char* name,
                                           const double* data, size_t len);
/** @brief Set a string array parameter (replaces stored pointer + length). */
NROS_PUBLIC
nros_ret_t nros_parameter_set_string_array(struct nros_parameter_server_t* server, const char* name,
                                           const char* const* data, size_t len);

/**
 * @brief Check if a parameter exists.
 *
 * @param server Pointer to an initialized parameter server.
 * @param name   Parameter name.
 * @return @c true if the parameter exists, @c false otherwise.
 */
NROS_PUBLIC bool nros_parameter_has(const struct nros_parameter_server_t* server, const char* name);

/**
 * @brief Get the type of a parameter.
 *
 * @param server Pointer to an initialized parameter server.
 * @param name   Parameter name.
 * @return Parameter type, or `NROS_PARAMETER_NOT_SET` if not found.
 */
NROS_PUBLIC
enum nros_parameter_type_t nros_parameter_get_type(const struct nros_parameter_server_t* server,
                                                   const char* name);

/**
 * @brief Get the number of declared parameters.
 *
 * @param server Pointer to an initialized parameter server.
 * @return Number of parameters.
 */
NROS_PUBLIC size_t nros_parameter_server_get_count(const struct nros_parameter_server_t* server);

/**
 * @brief Finalise a parameter server.
 *
 * @param server Pointer to an initialized parameter server.
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC nros_ret_t nros_parameter_server_fini(struct nros_parameter_server_t* server);

/* ===================================================================
 * Service-Backed Parameter API (requires NROS_PARAM_SERVICES feature)
 *
 * These functions operate on the nros-params::ParameterServer owned by
 * the Executor. After calling nros_executor_register_parameter_services,
 * declared parameters are visible to `ros2 param list /<node>`.
 *
 * Only available when nros-c is built with the `param-services` Cargo
 * feature (requires alloc).
 * =================================================================== */

struct nros_executor_t;
/* `struct nros_node_t` is NOT forward-declared here: it is fully defined in
 * <nros/nros_generated.h>, which the <nros/types.h> include at the top of this
 * file already brings in. A redundant forward declaration would also register
 * as a NEW TYPE on the generated-C surface (`check-codegen-version-surface`),
 * which would demand an NROS_CODEGEN_VERSION bump for a declaration that
 * changes nothing. */

/**
 * @brief Register the 6 ROS 2 parameter services on the executor's node.
 *
 * Creates service servers for:
 *   - `~/get_parameters`
 *   - `~/set_parameters`
 *   - `~/set_parameters_atomically`
 *   - `~/list_parameters`
 *   - `~/describe_parameters`
 *   - `~/get_parameter_types`
 *
 * After this call, parameters declared via
 * nros_executor_declare_param_*() are visible to `ros2 param` tooling.
 */
NROS_PUBLIC nros_ret_t nros_executor_register_parameter_services(struct nros_executor_t* executor);

/** @brief Declare a boolean parameter on the executor's PRIMARY node. */
NROS_PUBLIC nros_ret_t nros_executor_declare_param_bool(struct nros_executor_t* executor,
                                                        const char* name, bool value);
/** @brief Declare an integer parameter on the executor's PRIMARY node. */
NROS_PUBLIC nros_ret_t nros_executor_declare_param_integer(struct nros_executor_t* executor,
                                                           const char* name, int64_t value);
/** @brief Declare a double parameter on the executor's PRIMARY node. */
NROS_PUBLIC nros_ret_t nros_executor_declare_param_double(struct nros_executor_t* executor,
                                                          const char* name, double value);
/** @brief Declare a string parameter on the executor's PRIMARY node. */
NROS_PUBLIC nros_ret_t nros_executor_declare_param_string(struct nros_executor_t* executor,
                                                          const char* name, const char* value);

/** @brief Get a boolean parameter from the executor's PRIMARY node. */
NROS_PUBLIC nros_ret_t nros_executor_get_param_bool(struct nros_executor_t* executor,
                                                    const char* name, bool* out_value);
/** @brief Get an integer parameter from the executor's PRIMARY node. */
NROS_PUBLIC nros_ret_t nros_executor_get_param_integer(struct nros_executor_t* executor,
                                                       const char* name, int64_t* out_value);
/** @brief Get a double parameter from the executor's PRIMARY node. */
NROS_PUBLIC nros_ret_t nros_executor_get_param_double(struct nros_executor_t* executor,
                                                      const char* name, double* out_value);
/** @brief Get a string parameter into a caller-provided null-terminated buffer. */
NROS_PUBLIC nros_ret_t nros_executor_get_param_string(struct nros_executor_t* executor,
                                                      const char* name, char* out_value,
                                                      size_t max_len);

/** @brief Set a boolean parameter on the executor's PRIMARY node. */
NROS_PUBLIC nros_ret_t nros_executor_set_param_bool(struct nros_executor_t* executor,
                                                    const char* name, bool value);
/** @brief Set an integer parameter on the executor's PRIMARY node. */
NROS_PUBLIC nros_ret_t nros_executor_set_param_integer(struct nros_executor_t* executor,
                                                       const char* name, int64_t value);
/** @brief Set a double parameter on the executor's PRIMARY node. */
NROS_PUBLIC nros_ret_t nros_executor_set_param_double(struct nros_executor_t* executor,
                                                      const char* name, double value);
/** @brief Set a string parameter on the executor's PRIMARY node. */
NROS_PUBLIC nros_ret_t nros_executor_set_param_string(struct nros_executor_t* executor,
                                                      const char* name, const char* value);

/** @brief Check if a parameter exists on the executor's PRIMARY node. */
NROS_PUBLIC bool nros_executor_has_param(struct nros_executor_t* executor, const char* name);

/**
 * @brief May a set DECLARE a name the PRIMARY node never declared?
 *
 * Upstream's `allow_undeclared_parameters` node option, off by default. With
 * it off, nros_executor_set_param_*() on an undeclared name is
 * NROS_RET_NOT_FOUND and nothing is created.
 */
NROS_PUBLIC nros_ret_t nros_executor_allow_undeclared_parameters(struct nros_executor_t* executor,
                                                                 bool allow);

/* -------------------------------------------------------------------
 * Per-node spellings (phase-426 W5)
 *
 * The parameter store is keyed by NODE, because upstream's is: an image
 * composes several nodes onto one executor, and `/talker`'s `rate` is not
 * `/listener`'s. The functions above name the executor's PRIMARY node, which
 * is what a single-node image means and what
 * nros_executor_register_parameter_services() publishes. The `_on` spellings
 * below name any node bound to this executor by
 * nros_executor_node_init(); they are the C mirror of Rust's
 * `Executor::declare_parameter_on` / `_set_parameter_on` / `_get_parameter_on`
 * pair-per-method, and they point at the SAME table, so C and C++ cannot
 * disagree about what a node's parameters are.
 *
 * `node` must be INITIALISED and bound to `executor`; anything else is
 * NROS_RET_INVALID_ARGUMENT (false for the `has` / predicate form), never a
 * silent fallback to the primary node.
 *
 * These prototypes spell both handles as TYPEDEFS (`nros_executor_t*`,
 * `const nros_node_t*`) where the block above writes `struct nros_executor_t*`.
 * Both types are fully defined by <nros/nros_generated.h>, which the
 * <nros/types.h> include at the top of this file pulls in, so the two spellings
 * are the same type. The difference is `check-codegen-version-surface`: it
 * treats any `struct` keyword in a declaration as a TYPE DECLARATION and keys
 * the entry on the first tracked name inside it, so `struct nros_node_t* node`
 * in a prototype puts `nros_node_t` on the generated-C surface from this header
 * and demands an NROS_CODEGEN_VERSION bump. Nothing generated changed -- these
 * are new functions over an existing type, and no generated tree includes this
 * header -- so bumping would record a move that did not happen. Do not
 * reintroduce the `struct` keyword here for consistency with the block above.
 *
 * `enum` IS THE SAME TRAP, and phase-417 W4.a walked into it: the checker's
 * pattern is `typedef|struct|enum|union`, so `enum nros_parameter_type_t
 * nros_executor_get_param_type_on(..., const nros_node_t* node, ...)` is read
 * as a type declaration of `nros_node_t` exactly as a `struct` keyword would
 * be. Both names are typedefs in <nros/nros_generated.h>, so the bare spelling
 * is the same type -- write `nros_parameter_type_t`, never `enum
 * nros_parameter_type_t`, in every prototype below.
 * ------------------------------------------------------------------- */

/** @brief Declare a boolean parameter on @p node. */
NROS_PUBLIC nros_ret_t nros_executor_declare_param_bool_on(nros_executor_t* executor,
                                                           const nros_node_t* node,
                                                           const char* name, bool value);
/** @brief Declare an integer parameter on @p node. */
NROS_PUBLIC nros_ret_t nros_executor_declare_param_integer_on(nros_executor_t* executor,
                                                              const nros_node_t* node,
                                                              const char* name, int64_t value);
/** @brief Declare a double parameter on @p node. */
NROS_PUBLIC nros_ret_t nros_executor_declare_param_double_on(nros_executor_t* executor,
                                                             const nros_node_t* node,
                                                             const char* name, double value);
/** @brief Declare a string parameter on @p node. */
NROS_PUBLIC nros_ret_t nros_executor_declare_param_string_on(nros_executor_t* executor,
                                                             const nros_node_t* node,
                                                             const char* name, const char* value);

/** @brief Get a boolean parameter from @p node. */
NROS_PUBLIC nros_ret_t nros_executor_get_param_bool_on(nros_executor_t* executor,
                                                       const nros_node_t* node, const char* name,
                                                       bool* out_value);
/** @brief Get an integer parameter from @p node. */
NROS_PUBLIC nros_ret_t nros_executor_get_param_integer_on(nros_executor_t* executor,
                                                          const nros_node_t* node, const char* name,
                                                          int64_t* out_value);
/** @brief Get a double parameter from @p node. */
NROS_PUBLIC nros_ret_t nros_executor_get_param_double_on(nros_executor_t* executor,
                                                         const nros_node_t* node, const char* name,
                                                         double* out_value);
/** @brief Get a string parameter from @p node into a caller-provided buffer. */
NROS_PUBLIC nros_ret_t nros_executor_get_param_string_on(nros_executor_t* executor,
                                                         const nros_node_t* node, const char* name,
                                                         char* out_value, size_t max_len);

/** @brief Set a boolean parameter on @p node. */
NROS_PUBLIC nros_ret_t nros_executor_set_param_bool_on(nros_executor_t* executor,
                                                       const nros_node_t* node, const char* name,
                                                       bool value);
/** @brief Set an integer parameter on @p node. */
NROS_PUBLIC nros_ret_t nros_executor_set_param_integer_on(nros_executor_t* executor,
                                                          const nros_node_t* node, const char* name,
                                                          int64_t value);
/** @brief Set a double parameter on @p node. */
NROS_PUBLIC nros_ret_t nros_executor_set_param_double_on(nros_executor_t* executor,
                                                         const nros_node_t* node, const char* name,
                                                         double value);
/** @brief Set a string parameter on @p node. */
NROS_PUBLIC nros_ret_t nros_executor_set_param_string_on(nros_executor_t* executor,
                                                         const nros_node_t* node, const char* name,
                                                         const char* value);

/** @brief Check if a parameter exists on @p node. */
NROS_PUBLIC bool nros_executor_has_param_on(nros_executor_t* executor, const nros_node_t* node,
                                            const char* name);

/** @brief nros_executor_allow_undeclared_parameters() for @p node. Per node,
 *         as upstream: switching it on for one node leaves its siblings
 *         refusing. */
NROS_PUBLIC nros_ret_t nros_executor_allow_undeclared_parameters_on(nros_executor_t* executor,
                                                                    const nros_node_t* node,
                                                                    bool allow);

/* -------------------------------------------------------------------
 * Descriptors, undeclare, type query, listing and the on-set hook
 * (phase-417 W4.a)
 *
 * rclc attaches metadata AFTER the declaration -- rclc_add_parameter_description,
 * rclc_add_parameter_constraint_double / _integer, rclc_set_parameter_read_only --
 * and undeclares with rclc_delete_parameter. These are the same verbs against
 * the executor's store, so `ros2 param describe` answers with what a C caller
 * attached instead of the empty descriptor it used to send for everything C
 * declared.
 *
 * They are deliberately NOT on the legacy nros_parameter_server_t above: that
 * store is disjoint from the one the six rcl_interfaces service servers read
 * (issue 0793), and a descriptor surface on it would be a second answer to
 * "what is this parameter's range". Whether the legacy family is retired or
 * re-pointed is phase-417 W2.a's decision; nothing here depends on it.
 *
 * ## How a descriptor crosses this boundary without allocating
 *
 * There is no descriptor STRUCT, on purpose. Text goes IN as a borrowed
 * `const char*` and comes back OUT in a caller-owned `char*` of stated
 * capacity -- the shape nros_executor_get_param_string() already has --
 * and the scalars are ordinary out-params, each of which may be NULL when the
 * caller does not want it. A struct would either carry pointers into the
 * store (a borrow no C caller can honour) or an inline buffer, which would put
 * NROS_MAX_PARAM_DESCRIPTION_LEN into a public ABI and make a build knob a
 * layout. Nothing on either side of these calls allocates.
 * ------------------------------------------------------------------- */

/** @brief Attach a description and free-text constraints to a parameter
 *         declared on the PRIMARY node (rclc `rclc_add_parameter_description`).
 *
 * Either text may be NULL, meaning "clear it".
 * @retval NROS_RET_NOT_FOUND if the parameter is not declared. */
NROS_PUBLIC nros_ret_t nros_executor_add_param_description(nros_executor_t* executor,
                                                           const char* name,
                                                           const char* description,
                                                           const char* additional_constraints);
/** @brief nros_executor_add_param_description() for @p node. */
NROS_PUBLIC nros_ret_t nros_executor_add_param_description_on(nros_executor_t* executor,
                                                              const nros_node_t* node,
                                                              const char* name,
                                                              const char* description,
                                                              const char* additional_constraints);

/** @brief Mark a declared parameter read-only on the PRIMARY node (rclc
 *         `rclc_set_parameter_read_only`). Every later write, local or over
 *         `~/set_parameters`, is refused. */
NROS_PUBLIC nros_ret_t nros_executor_set_param_read_only(nros_executor_t* executor,
                                                         const char* name, bool read_only);
/** @brief nros_executor_set_param_read_only() for @p node. */
NROS_PUBLIC nros_ret_t nros_executor_set_param_read_only_on(nros_executor_t* executor,
                                                            const nros_node_t* node,
                                                            const char* name, bool read_only);

/** @brief Attach an integer range to a declared parameter on the PRIMARY node
 *         (rclc `rclc_add_parameter_constraint_integer`).
 *
 * @retval NROS_RET_INVALID_ARGUMENT for an ill-formed range (from > to, or a
 *         negative step), or one the parameter's CURRENT value does not
 *         satisfy -- attaching either would advertise a bound no set could
 *         have produced. */
NROS_PUBLIC nros_ret_t nros_executor_add_param_constraint_integer(nros_executor_t* executor,
                                                                  const char* name,
                                                                  int64_t from_value,
                                                                  int64_t to_value, int64_t step);
/** @brief nros_executor_add_param_constraint_integer() for @p node. */
NROS_PUBLIC nros_ret_t nros_executor_add_param_constraint_integer_on(
    nros_executor_t* executor, const nros_node_t* node, const char* name, int64_t from_value,
    int64_t to_value, int64_t step);

/** @brief Attach a floating-point range to a declared parameter on the PRIMARY
 *         node (rclc `rclc_add_parameter_constraint_double`). */
NROS_PUBLIC nros_ret_t nros_executor_add_param_constraint_double(nros_executor_t* executor,
                                                                 const char* name,
                                                                 double from_value, double to_value,
                                                                 double step);
/** @brief nros_executor_add_param_constraint_double() for @p node. */
NROS_PUBLIC nros_ret_t nros_executor_add_param_constraint_double_on(nros_executor_t* executor,
                                                                    const nros_node_t* node,
                                                                    const char* name,
                                                                    double from_value,
                                                                    double to_value, double step);

/** @brief Undeclare a parameter on the PRIMARY node (rclc
 *         `rclc_delete_parameter`, rclcpp `undeclare_parameter`). The slot is
 *         freed for a later declaration.
 * @retval NROS_RET_NOT_FOUND if the node never declared it. */
NROS_PUBLIC nros_ret_t nros_executor_delete_param(nros_executor_t* executor, const char* name);
/** @brief nros_executor_delete_param() for @p node. */
NROS_PUBLIC nros_ret_t nros_executor_delete_param_on(nros_executor_t* executor,
                                                     const nros_node_t* node, const char* name);

/** @brief The declared TYPE of a parameter on the PRIMARY node, or
 *         `NROS_PARAMETER_NOT_SET` when it is not declared.
 *
 * nros_parameter_get_type() answers the same question about the LEGACY store,
 * which is not the one `ros2 param get` reads. This is the executor-store
 * answer, and the local form of what `~/get_parameter_types` serves. */
NROS_PUBLIC nros_parameter_type_t nros_executor_get_param_type(nros_executor_t* executor,
                                                               const char* name);
/** @brief nros_executor_get_param_type() for @p node. */
NROS_PUBLIC nros_parameter_type_t nros_executor_get_param_type_on(nros_executor_t* executor,
                                                                  const nros_node_t* node,
                                                                  const char* name);

/** @brief Read a declared parameter's descriptor into caller-owned storage
 *         (rclcpp `describe_parameter`), on the PRIMARY node.
 *
 * Any out-param may be NULL. A parameter declared with no descriptor answers
 * NROS_RET_OK with the defaults -- empty text, not read-only -- because that
 * IS its description, and it is the same answer `~/describe_parameters` gives.
 *
 * @retval NROS_RET_NOT_FOUND the parameter is not declared on this node.
 * @retval NROS_RET_FULL      a text buffer was too small. The text is still
 *                            written, truncated at a character boundary and
 *                            null-terminated. */
NROS_PUBLIC nros_ret_t nros_executor_describe_param(nros_executor_t* executor, const char* name,
                                                    char* out_description, size_t description_len,
                                                    char* out_constraints, size_t constraints_len,
                                                    bool* out_read_only,
                                                    nros_parameter_type_t* out_type);
/** @brief nros_executor_describe_param() for @p node. */
NROS_PUBLIC nros_ret_t nros_executor_describe_param_on(
    nros_executor_t* executor, const nros_node_t* node, const char* name, char* out_description,
    size_t description_len, char* out_constraints, size_t constraints_len, bool* out_read_only,
    nros_parameter_type_t* out_type);

/** @brief The integer range attached to a parameter on the PRIMARY node.
 *
 * @retval NROS_RET_NOT_FOUND the parameter is undeclared OR carries no integer
 *         range. The two are the same answer to "what may I write", and
 *         distinguishing them would need an out-param nobody would read. */
NROS_PUBLIC nros_ret_t nros_executor_get_param_integer_range(nros_executor_t* executor,
                                                             const char* name, int64_t* out_from,
                                                             int64_t* out_to, int64_t* out_step);
/** @brief nros_executor_get_param_integer_range() for @p node. */
NROS_PUBLIC nros_ret_t nros_executor_get_param_integer_range_on(nros_executor_t* executor,
                                                                const nros_node_t* node,
                                                                const char* name, int64_t* out_from,
                                                                int64_t* out_to, int64_t* out_step);

/** @brief The floating-point range attached to a parameter on the PRIMARY
 *         node. See nros_executor_get_param_integer_range(). */
NROS_PUBLIC nros_ret_t nros_executor_get_param_double_range(nros_executor_t* executor,
                                                            const char* name, double* out_from,
                                                            double* out_to, double* out_step);
/** @brief nros_executor_get_param_double_range() for @p node. */
NROS_PUBLIC nros_ret_t nros_executor_get_param_double_range_on(nros_executor_t* executor,
                                                               const nros_node_t* node,
                                                               const char* name, double* out_from,
                                                               double* out_to, double* out_step);

/** @brief Enumerate the PRIMARY node's declared parameter names, prefix-filtered
 *         (rclcpp `list_parameters`, `~/list_parameters`).
 *
 * Names land in a caller-owned RECTANGLE -- @p max_names rows of
 * @p name_stride bytes -- because a list of strings cannot cross this boundary
 * any other way without an allocator. `*out_count` always receives the TOTAL
 * that matched, whether or not they fit, so the two-call "ask for the size,
 * then read" shape works: pass @p max_names 0 to count.
 *
 * @param prefix  Prefix to filter by; NULL or "" lists everything.
 * @retval NROS_RET_FULL more matched than fit, or a name was longer than
 *         @p name_stride. The rows that did fit are still written. */
NROS_PUBLIC nros_ret_t nros_executor_list_params(nros_executor_t* executor, const char* prefix,
                                                 char* out_names, size_t name_stride,
                                                 size_t max_names, size_t* out_count);
/** @brief nros_executor_list_params() for @p node. */
NROS_PUBLIC nros_ret_t nros_executor_list_params_on(nros_executor_t* executor,
                                                    const nros_node_t* node, const char* prefix,
                                                    char* out_names, size_t name_stride,
                                                    size_t max_names, size_t* out_count);

/** @brief Register an accept/reject hook for writes to the PRIMARY node
 *         (rclcpp `add_on_set_parameters_callback`).
 *
 * The callback returns `false` to REFUSE the write, which is the contract
 * nros_parameter_server_set_callback() has had since phase 84 -- on the LEGACY
 * store, which no service reads, so it fired for nobody (issue 0793). This is
 * the same callback TYPE on the store `ros2 param set` reaches. One typedef,
 * two stores, no third spelling.
 *
 * The hook runs AFTER the store's own rules, so it never sees a write that
 * read-only, the declared type or a range already refused. Array values reach
 * it as their TYPE with an empty `array_value`: the proposed elements are not
 * yet in the store and this ABI cannot state a borrow for them.
 *
 * @param out_handle  Receives the token nros_executor_remove_param_callback()
 *                    takes; NULL if the caller never intends to unregister.
 * @retval NROS_RET_FULL all of the store's on-set slots are taken -- a
 *         refusal, never a silent eviction of somebody else's hook. */
NROS_PUBLIC nros_ret_t nros_executor_set_param_callback(nros_executor_t* executor,
                                                        nros_parameter_callback_t callback,
                                                        void* context, uint16_t* out_handle);
/** @brief nros_executor_set_param_callback() for @p node. */
NROS_PUBLIC nros_ret_t nros_executor_set_param_callback_on(nros_executor_t* executor,
                                                           const nros_node_t* node,
                                                           nros_parameter_callback_t callback,
                                                           void* context, uint16_t* out_handle);

/** @brief Unregister a hook (rclcpp `remove_on_set_parameters_callback`).
 * @retval NROS_RET_NOT_FOUND the handle names no registration. */
NROS_PUBLIC nros_ret_t nros_executor_remove_param_callback(nros_executor_t* executor,
                                                           uint16_t handle);

#ifdef __cplusplus
}
#endif

#endif /* NROS_PARAMETER_H */
