/*
 * Phase 379 W5 — the parameter family was renamed `nros_param_*` ->
 * `nros_parameter_*` (ledger row `c:parameter_server_t`). This TU pins BOTH
 * halves of that change, the way `executor_verb_aliases.c` pins issue 0338's:
 *
 *   1. every entry point exists under the NEW spelling, with the signature the
 *      header documents, and
 *   2. every OLD spelling still compiles, with that same signature, so a
 *      consumer gets a release to migrate.
 *
 * Compile-only (no main): the assertion is that these names resolve with these
 * types. Taking a function pointer forces a real lookup and a real signature
 * match — a forwarder whose argument list drifted from the function it forwards
 * to fails HERE rather than at some consumer's call site.
 *
 * The old half is `static inline` forwarders carrying NROS_DEPRECATED_MSG, so
 * naming them is supposed to warn. That warning is the point, and it is
 * asserted separately: `just check-c` compiles a one-line probe with
 * `-Werror=deprecated-declarations` and requires it to FAIL. Here the warning
 * would just be noise on a passing gate, so it is suppressed for that section
 * only.
 */

#include "nros/parameter.h"

/* 1. The renamed entry points exist. */
static struct nros_parameter_server_t (*const k_new_server_get_zero_initialized)(void) =
    nros_parameter_server_get_zero_initialized;
static nros_ret_t (*const k_new_server_init)(struct nros_parameter_server_t*,
                                             struct nros_parameter_t*,
                                             size_t) = nros_parameter_server_init;
static nros_ret_t (*const k_new_server_set_callback)(struct nros_parameter_server_t*,
                                                     nros_parameter_callback_t,
                                                     void*) = nros_parameter_server_set_callback;
static nros_ret_t (*const k_new_declare_bool)(struct nros_parameter_server_t*, const char*,
                                              bool) = nros_parameter_declare_bool;
static nros_ret_t (*const k_new_declare_integer)(struct nros_parameter_server_t*, const char*,
                                                 int64_t) = nros_parameter_declare_integer;
static nros_ret_t (*const k_new_declare_double)(struct nros_parameter_server_t*, const char*,
                                                double) = nros_parameter_declare_double;
static nros_ret_t (*const k_new_declare_string)(struct nros_parameter_server_t*, const char*,
                                                const char*) = nros_parameter_declare_string;
static nros_ret_t (*const k_new_get_bool)(const struct nros_parameter_server_t*, const char*,
                                          bool*) = nros_parameter_get_bool;
static nros_ret_t (*const k_new_get_integer)(const struct nros_parameter_server_t*, const char*,
                                             int64_t*) = nros_parameter_get_integer;
static nros_ret_t (*const k_new_get_double)(const struct nros_parameter_server_t*, const char*,
                                            double*) = nros_parameter_get_double;
static nros_ret_t (*const k_new_get_string)(const struct nros_parameter_server_t*, const char*,
                                            char*, size_t) = nros_parameter_get_string;
static nros_ret_t (*const k_new_set_bool)(struct nros_parameter_server_t*, const char*,
                                          bool) = nros_parameter_set_bool;
static nros_ret_t (*const k_new_set_integer)(struct nros_parameter_server_t*, const char*,
                                             int64_t) = nros_parameter_set_integer;
static nros_ret_t (*const k_new_set_double)(struct nros_parameter_server_t*, const char*,
                                            double) = nros_parameter_set_double;
static nros_ret_t (*const k_new_set_string)(struct nros_parameter_server_t*, const char*,
                                            const char*) = nros_parameter_set_string;
static nros_ret_t (*const k_new_declare_byte_array)(struct nros_parameter_server_t*, const char*,
                                                    const uint8_t*,
                                                    size_t) = nros_parameter_declare_byte_array;
static nros_ret_t (*const k_new_declare_bool_array)(struct nros_parameter_server_t*, const char*,
                                                    const bool*,
                                                    size_t) = nros_parameter_declare_bool_array;
static nros_ret_t (*const k_new_declare_integer_array)(struct nros_parameter_server_t*, const char*,
                                                       const int64_t*, size_t) =
    nros_parameter_declare_integer_array;
static nros_ret_t (*const k_new_declare_double_array)(struct nros_parameter_server_t*, const char*,
                                                      const double*,
                                                      size_t) = nros_parameter_declare_double_array;
static nros_ret_t (*const k_new_declare_string_array)(struct nros_parameter_server_t*, const char*,
                                                      const char* const*,
                                                      size_t) = nros_parameter_declare_string_array;
static nros_ret_t (*const k_new_get_byte_array)(const struct nros_parameter_server_t*, const char*,
                                                const uint8_t**,
                                                size_t*) = nros_parameter_get_byte_array;
static nros_ret_t (*const k_new_get_bool_array)(const struct nros_parameter_server_t*, const char*,
                                                const bool**,
                                                size_t*) = nros_parameter_get_bool_array;
static nros_ret_t (*const k_new_get_integer_array)(const struct nros_parameter_server_t*,
                                                   const char*, const int64_t**,
                                                   size_t*) = nros_parameter_get_integer_array;
static nros_ret_t (*const k_new_get_double_array)(const struct nros_parameter_server_t*,
                                                  const char*, const double**,
                                                  size_t*) = nros_parameter_get_double_array;
static nros_ret_t (*const k_new_get_string_array)(const struct nros_parameter_server_t*,
                                                  const char*, const char* const**,
                                                  size_t*) = nros_parameter_get_string_array;
static nros_ret_t (*const k_new_set_byte_array)(struct nros_parameter_server_t*, const char*,
                                                const uint8_t*,
                                                size_t) = nros_parameter_set_byte_array;
static nros_ret_t (*const k_new_set_bool_array)(struct nros_parameter_server_t*, const char*,
                                                const bool*,
                                                size_t) = nros_parameter_set_bool_array;
static nros_ret_t (*const k_new_set_integer_array)(struct nros_parameter_server_t*, const char*,
                                                   const int64_t*,
                                                   size_t) = nros_parameter_set_integer_array;
static nros_ret_t (*const k_new_set_double_array)(struct nros_parameter_server_t*, const char*,
                                                  const double*,
                                                  size_t) = nros_parameter_set_double_array;
static nros_ret_t (*const k_new_set_string_array)(struct nros_parameter_server_t*, const char*,
                                                  const char* const*,
                                                  size_t) = nros_parameter_set_string_array;
static bool (*const k_new_has)(const struct nros_parameter_server_t*,
                               const char*) = nros_parameter_has;
static enum nros_parameter_type_t (*const k_new_get_type)(const struct nros_parameter_server_t*,
                                                          const char*) = nros_parameter_get_type;
static size_t (*const k_new_server_get_count)(const struct nros_parameter_server_t*) =
    nros_parameter_server_get_count;
static nros_ret_t (*const k_new_server_fini)(struct nros_parameter_server_t*) =
    nros_parameter_server_fini;

/* 2. The deprecated spellings still resolve, with the same signatures.
 *
 * The parameters are written with the deprecated TYPE aliases
 * (`nros_param_server_t`) on purpose: those are part of the compatibility
 * surface too, and this is where a missing one would show up. Note the aliases
 * are typedefs, so the tagged spelling `struct nros_param_server_t` is NOT
 * available and this file cannot use it — that limit is documented in
 * `nros/parameter.h` and is deliberate, not an oversight here.
 */
#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wdeprecated-declarations"
#endif
static nros_param_server_t (*const k_old_server_get_zero_initialized)(void) =
    nros_param_server_get_zero_initialized;
static nros_ret_t (*const k_old_server_init)(nros_param_server_t*, struct nros_parameter_t*,
                                             size_t) = nros_param_server_init;
static nros_ret_t (*const k_old_server_set_callback)(nros_param_server_t*,
                                                     nros_parameter_callback_t,
                                                     void*) = nros_param_server_set_callback;
static nros_ret_t (*const k_old_declare_bool)(nros_param_server_t*, const char*,
                                              bool) = nros_param_declare_bool;
static nros_ret_t (*const k_old_declare_integer)(nros_param_server_t*, const char*,
                                                 int64_t) = nros_param_declare_integer;
static nros_ret_t (*const k_old_declare_double)(nros_param_server_t*, const char*,
                                                double) = nros_param_declare_double;
static nros_ret_t (*const k_old_declare_string)(nros_param_server_t*, const char*,
                                                const char*) = nros_param_declare_string;
static nros_ret_t (*const k_old_get_bool)(const nros_param_server_t*, const char*,
                                          bool*) = nros_param_get_bool;
static nros_ret_t (*const k_old_get_integer)(const nros_param_server_t*, const char*,
                                             int64_t*) = nros_param_get_integer;
static nros_ret_t (*const k_old_get_double)(const nros_param_server_t*, const char*,
                                            double*) = nros_param_get_double;
static nros_ret_t (*const k_old_get_string)(const nros_param_server_t*, const char*, char*,
                                            size_t) = nros_param_get_string;
static nros_ret_t (*const k_old_set_bool)(nros_param_server_t*, const char*,
                                          bool) = nros_param_set_bool;
static nros_ret_t (*const k_old_set_integer)(nros_param_server_t*, const char*,
                                             int64_t) = nros_param_set_integer;
static nros_ret_t (*const k_old_set_double)(nros_param_server_t*, const char*,
                                            double) = nros_param_set_double;
static nros_ret_t (*const k_old_set_string)(nros_param_server_t*, const char*,
                                            const char*) = nros_param_set_string;
static nros_ret_t (*const k_old_declare_byte_array)(nros_param_server_t*, const char*,
                                                    const uint8_t*,
                                                    size_t) = nros_param_declare_byte_array;
static nros_ret_t (*const k_old_declare_bool_array)(nros_param_server_t*, const char*, const bool*,
                                                    size_t) = nros_param_declare_bool_array;
static nros_ret_t (*const k_old_declare_integer_array)(nros_param_server_t*, const char*,
                                                       const int64_t*,
                                                       size_t) = nros_param_declare_integer_array;
static nros_ret_t (*const k_old_declare_double_array)(nros_param_server_t*, const char*,
                                                      const double*,
                                                      size_t) = nros_param_declare_double_array;
static nros_ret_t (*const k_old_declare_string_array)(nros_param_server_t*, const char*,
                                                      const char* const*,
                                                      size_t) = nros_param_declare_string_array;
static nros_ret_t (*const k_old_get_byte_array)(const nros_param_server_t*, const char*,
                                                const uint8_t**,
                                                size_t*) = nros_param_get_byte_array;
static nros_ret_t (*const k_old_get_bool_array)(const nros_param_server_t*, const char*,
                                                const bool**, size_t*) = nros_param_get_bool_array;
static nros_ret_t (*const k_old_get_integer_array)(const nros_param_server_t*, const char*,
                                                   const int64_t**,
                                                   size_t*) = nros_param_get_integer_array;
static nros_ret_t (*const k_old_get_double_array)(const nros_param_server_t*, const char*,
                                                  const double**,
                                                  size_t*) = nros_param_get_double_array;
static nros_ret_t (*const k_old_get_string_array)(const nros_param_server_t*, const char*,
                                                  const char* const**,
                                                  size_t*) = nros_param_get_string_array;
static nros_ret_t (*const k_old_set_byte_array)(nros_param_server_t*, const char*, const uint8_t*,
                                                size_t) = nros_param_set_byte_array;
static nros_ret_t (*const k_old_set_bool_array)(nros_param_server_t*, const char*, const bool*,
                                                size_t) = nros_param_set_bool_array;
static nros_ret_t (*const k_old_set_integer_array)(nros_param_server_t*, const char*,
                                                   const int64_t*,
                                                   size_t) = nros_param_set_integer_array;
static nros_ret_t (*const k_old_set_double_array)(nros_param_server_t*, const char*, const double*,
                                                  size_t) = nros_param_set_double_array;
static nros_ret_t (*const k_old_set_string_array)(nros_param_server_t*, const char*,
                                                  const char* const*,
                                                  size_t) = nros_param_set_string_array;
static bool (*const k_old_has)(const nros_param_server_t*, const char*) = nros_param_has;
static enum nros_parameter_type_t (*const k_old_get_type)(const nros_param_server_t*,
                                                          const char*) = nros_param_get_type;
static size_t (*const k_old_server_get_count)(const nros_param_server_t*) =
    nros_param_server_get_count;
static nros_ret_t (*const k_old_server_fini)(nros_param_server_t*) = nros_param_server_fini;
#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic pop
#endif

/* 3. The deprecated TYPE aliases name the same types as the live spellings.
 *    A typedef cannot warn (see nros/parameter.h), so this is all the checking
 *    there is for them — which is exactly why it is written down. */
static nros_param_callback_t k_old_callback_t;
static nros_param_array_t k_old_array_t;
static nros_param_server_state_t k_old_state_t;
static nros_parameter_callback_t* const k_cb_same = &k_old_callback_t;
static struct nros_parameter_array_t* const k_arr_same = &k_old_array_t;
static enum nros_parameter_server_state_t* const k_state_same = &k_old_state_t;

/* Silence "defined but not used" without needing a main(). */
const void* nros_param_name_alias_probe(void);
const void* nros_param_name_alias_probe(void) {
    (void)k_new_server_get_zero_initialized;
    (void)k_old_server_get_zero_initialized;
    (void)k_new_server_init;
    (void)k_old_server_init;
    (void)k_new_server_set_callback;
    (void)k_old_server_set_callback;
    (void)k_new_declare_bool;
    (void)k_old_declare_bool;
    (void)k_new_declare_integer;
    (void)k_old_declare_integer;
    (void)k_new_declare_double;
    (void)k_old_declare_double;
    (void)k_new_declare_string;
    (void)k_old_declare_string;
    (void)k_new_get_bool;
    (void)k_old_get_bool;
    (void)k_new_get_integer;
    (void)k_old_get_integer;
    (void)k_new_get_double;
    (void)k_old_get_double;
    (void)k_new_get_string;
    (void)k_old_get_string;
    (void)k_new_set_bool;
    (void)k_old_set_bool;
    (void)k_new_set_integer;
    (void)k_old_set_integer;
    (void)k_new_set_double;
    (void)k_old_set_double;
    (void)k_new_set_string;
    (void)k_old_set_string;
    (void)k_new_declare_byte_array;
    (void)k_old_declare_byte_array;
    (void)k_new_declare_bool_array;
    (void)k_old_declare_bool_array;
    (void)k_new_declare_integer_array;
    (void)k_old_declare_integer_array;
    (void)k_new_declare_double_array;
    (void)k_old_declare_double_array;
    (void)k_new_declare_string_array;
    (void)k_old_declare_string_array;
    (void)k_new_get_byte_array;
    (void)k_old_get_byte_array;
    (void)k_new_get_bool_array;
    (void)k_old_get_bool_array;
    (void)k_new_get_integer_array;
    (void)k_old_get_integer_array;
    (void)k_new_get_double_array;
    (void)k_old_get_double_array;
    (void)k_new_get_string_array;
    (void)k_old_get_string_array;
    (void)k_new_set_byte_array;
    (void)k_old_set_byte_array;
    (void)k_new_set_bool_array;
    (void)k_old_set_bool_array;
    (void)k_new_set_integer_array;
    (void)k_old_set_integer_array;
    (void)k_new_set_double_array;
    (void)k_old_set_double_array;
    (void)k_new_set_string_array;
    (void)k_old_set_string_array;
    (void)k_new_has;
    (void)k_old_has;
    (void)k_new_get_type;
    (void)k_old_get_type;
    (void)k_new_server_get_count;
    (void)k_old_server_get_count;
    (void)k_new_server_fini;
    (void)k_old_server_fini;
    (void)k_cb_same;
    (void)k_arr_same;
    (void)k_state_same;
    return (const void*)k_new_server_fini;
}
