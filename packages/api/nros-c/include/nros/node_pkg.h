/**
 * @file component.h
 * @ingroup grp_node
 * @brief Component-mode declarations for metadata and generated runtimes.
 *
 * Component packages do not define `main()`. They export a per-pkg mangled
 * `__nros_component_<pkg>_register` symbol (Phase 212.M.5.a.1) so multiple
 * components can link into one binary; the host metadata tool or generated
 * runtime supplies a context whose operations record declarations or
 * instantiate executor nodes. Codegen resolves the symbol name from each
 * package's metadata — there is no hardcoded bare-name constant.
 */

#ifndef NROS_NODE_PKG_H
#define NROS_NODE_PKG_H

#include <stddef.h>
#include <stdint.h>

#include "nros/visibility.h"

typedef int nros_ret_t;

#ifndef NROS_RET_OK
#define NROS_RET_OK 0
#endif

#ifndef NROS_RET_INVALID_ARGUMENT
#define NROS_RET_INVALID_ARGUMENT -3
#endif

#ifdef __cplusplus
extern "C" {
#endif

/* Phase 257 (Stage-3b) — the declarative node-registration seam
 * (nros_node_context_t, nros_declared_*, NROS_NODE_REGISTER / NROS_COMPONENT,
 * the NodeEntityDescriptor structs) is retired; the EntryNodeRuntime
 * interpreter that consumed it is gone. Typed C components use
 * `<nros/component.h>` (NROS_C_COMPONENT). This header now only carries the
 * shared `nros_ret_t` + `NROS_RET_*` C return surface above. */

#ifdef __cplusplus
}
#endif

#endif /* NROS_NODE_PKG_H */
