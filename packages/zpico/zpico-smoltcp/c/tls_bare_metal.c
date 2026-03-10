/**
 * TLS platform symbols for bare-metal (smoltcp) targets.
 *
 * Implements the 9 zenoh-pico TLS platform functions using mbedTLS with
 * custom BIO callbacks that route through SmoltcpBridge's FFI exports.
 *
 * Modeled on zenoh-pico/src/system/unix/tls.c but adapted for bare-metal:
 * - No filesystem (base64-only certificate loading)
 * - No POSIX sockets (smoltcp staging buffers via FFI)
 * - Cooperative polling (smoltcp_poll_network() in handshake loop)
 * - Static TLS context pool (no heap for large structs)
 * - Custom allocator hooks (z_malloc/z_free)
 */

#include "zenoh-pico/config.h"

#if Z_FEATURE_LINK_TLS == 1

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include "mbedtls/base64.h"
#include "mbedtls/entropy.h"
#include "mbedtls/error.h"
#include "mbedtls/hmac_drbg.h"
#include "mbedtls/md.h"
#include "mbedtls/pk.h"
#include "mbedtls/platform.h"
#include "mbedtls/ssl.h"
#include "mbedtls/x509_crt.h"

#include "zenoh-pico/system/link/tls.h"
#include "zenoh-pico/system/link/tcp.h"
#include "zenoh-pico/link/config/tls.h"
#include "zenoh-pico/utils/logging.h"
#include "zenoh-pico/utils/pointers.h"

/* ============================================================================
 * SmoltcpBridge FFI imports (provided by zpico-smoltcp Rust code)
 * ============================================================================ */

extern int32_t smoltcp_socket_recv(int32_t handle, uint8_t* buf, size_t len);
extern int32_t smoltcp_socket_send(int32_t handle, const uint8_t* buf, size_t len);
extern void smoltcp_poll_network(void);
extern uint64_t smoltcp_clock_ms(void);

/* zenoh-pico memory allocation (provided by platform crates) */
extern void* z_malloc(size_t size);
extern void z_free(void* ptr);

/* ============================================================================
 * Configuration
 * ============================================================================ */

#ifndef ZPICO_SMOLTCP_MAX_TLS_SOCKETS
#define ZPICO_SMOLTCP_MAX_TLS_SOCKETS 1
#endif

#define Z_TLS_BASE64_MAX_VALUE_LEN (64 * 1024)
#define TLS_HANDSHAKE_TIMEOUT_MS 30000

/* ============================================================================
 * Static TLS context pool
 * ============================================================================ */

static _z_tls_context_t TLS_CONTEXT_POOL[ZPICO_SMOLTCP_MAX_TLS_SOCKETS];
static bool TLS_CONTEXT_ALLOCATED[ZPICO_SMOLTCP_MAX_TLS_SOCKETS];

/* ============================================================================
 * C stdlib stubs for bare-metal
 * ============================================================================ */

/* strstr — used by mbedTLS PEM/X.509 parsing */
__attribute__((weak)) char* strstr(const char* haystack, const char* needle) {
    if (!needle[0]) return (char*)haystack;
    for (; *haystack; haystack++) {
        const char *h = haystack, *n = needle;
        while (*h && *n && *h == *n) {
            h++;
            n++;
        }
        if (!*n) return (char*)haystack;
    }
    return NULL;
}

/* ============================================================================
 * Custom allocator for mbedTLS (compile-time macros via mbedtls_config.h)
 * ============================================================================ */

void* z_bare_metal_calloc(size_t n, size_t size) {
    size_t total = n * size;
    void* p = z_malloc(total);
    if (p) {
        memset(p, 0, total);
    }
    return p;
}

void z_bare_metal_free(void* ptr) {
    z_free(ptr);
}

/* ============================================================================
 * BIO callbacks for smoltcp
 * ============================================================================ */

static int _z_tls_bio_send_smoltcp(void* ctx, const unsigned char* buf, size_t len) {
    int8_t handle = *(int8_t*)ctx;
    smoltcp_poll_network();
    int32_t sent = smoltcp_socket_send((int32_t)handle, buf, len);
    if (sent <= 0) {
        return MBEDTLS_ERR_SSL_WANT_WRITE;
    }
    return (int)sent;
}

static int _z_tls_bio_recv_smoltcp(void* ctx, unsigned char* buf, size_t len) {
    int8_t handle = *(int8_t*)ctx;
    smoltcp_poll_network();
    int32_t received = smoltcp_socket_recv((int32_t)handle, buf, len);
    if (received <= 0) {
        return MBEDTLS_ERR_SSL_WANT_READ;
    }
    return (int)received;
}

/* ============================================================================
 * Base64 decoding helpers
 * ============================================================================ */

static z_result_t _z_tls_decode_base64(const char* label, const char* input, unsigned char** output,
                                       size_t* output_len) {
    if (input == NULL || label == NULL) {
        return _Z_ERR_GENERIC;
    }

    size_t input_len = strlen(input);
    if (input_len > Z_TLS_BASE64_MAX_VALUE_LEN) {
        return _Z_ERR_GENERIC;
    }

    size_t required = 0;
    int ret = mbedtls_base64_decode(NULL, 0, &required, (const unsigned char*)input, input_len);
    if (ret != 0 && ret != MBEDTLS_ERR_BASE64_BUFFER_TOO_SMALL) {
        return _Z_ERR_GENERIC;
    }

    size_t buffer_len = (required > 0) ? required : 1;
    unsigned char* buffer = (unsigned char*)z_malloc(buffer_len + 1);
    if (buffer == NULL) {
        return _Z_ERR_SYSTEM_OUT_OF_MEMORY;
    }

    ret = mbedtls_base64_decode(buffer, buffer_len, &required, (const unsigned char*)input,
                                input_len);
    if (ret != 0) {
        z_free(buffer);
        return _Z_ERR_GENERIC;
    }

    buffer[required] = '\0';
    *output = buffer;
    if (output_len != NULL) {
        *output_len = required;
    }
    return _Z_RES_OK;
}

static z_result_t _z_tls_parse_cert_from_base64(mbedtls_x509_crt* cert, const char* base64,
                                                const char* label) {
    unsigned char* decoded = NULL;
    size_t decoded_len = 0;
    z_result_t res = _z_tls_decode_base64(label, base64, &decoded, &decoded_len);
    if (res != _Z_RES_OK) {
        return res;
    }

    int ret = mbedtls_x509_crt_parse(cert, decoded, decoded_len + 1);
    z_free(decoded);
    if (ret != 0) {
        return _Z_ERR_GENERIC;
    }

    return _Z_RES_OK;
}

static z_result_t _z_tls_parse_key_from_base64(mbedtls_pk_context* key, const char* base64,
                                               const char* label, mbedtls_hmac_drbg_context* rng) {
    unsigned char* decoded = NULL;
    size_t decoded_len = 0;
    z_result_t res = _z_tls_decode_base64(label, base64, &decoded, &decoded_len);
    if (res != _Z_RES_OK) {
        return res;
    }

    int ret = mbedtls_pk_parse_key(key, decoded, decoded_len + 1, NULL, 0);
    z_free(decoded);
    if (ret != 0) {
        return _Z_ERR_GENERIC;
    }

    (void)rng;
    return _Z_RES_OK;
}

static bool _z_opt_is_true(const char* val) {
    if (val == NULL || val[0] == '\0') {
        return true;
    }
    char c = val[0];
    return !(c == '0' || c == 'n' || c == 'N' || c == 'f' || c == 'F');
}

/* ============================================================================
 * Certificate loading (base64 only — no filesystem on bare-metal)
 * ============================================================================ */

static z_result_t _z_tls_load_ca_certificate(_z_tls_context_t* ctx, const _z_str_intmap_t* config) {
    const char* ca_cert_base64 =
        _z_str_intmap_get(config, TLS_CONFIG_ROOT_CA_CERTIFICATE_BASE64_KEY);
    const char* ca_cert_file = _z_str_intmap_get(config, TLS_CONFIG_ROOT_CA_CERTIFICATE_KEY);

    /* On bare-metal, file paths are not supported */
    if (ca_cert_base64 == NULL && ca_cert_file == NULL) {
        _Z_ERROR("TLS requires 'root_ca_certificate_base64' to be set");
        return _Z_ERR_GENERIC;
    }

    if (ca_cert_file != NULL && ca_cert_base64 == NULL) {
        _Z_ERROR("TLS file-based certificates not supported on bare-metal. "
                 "Use root_ca_certificate_base64 instead.");
        return _Z_ERR_GENERIC;
    }

    if (ca_cert_base64 != NULL) {
        z_result_t res =
            _z_tls_parse_cert_from_base64(&ctx->_ca_cert, ca_cert_base64, "CA certificate");
        if (res != _Z_RES_OK) {
            return res;
        }
    }

    return _Z_RES_OK;
}

static z_result_t _z_tls_load_client_cert(_z_tls_context_t* ctx, const _z_str_intmap_t* config) {
    const char* key_base64 = _z_str_intmap_get(config, TLS_CONFIG_CONNECT_PRIVATE_KEY_BASE64_KEY);
    const char* cert_base64 = _z_str_intmap_get(config, TLS_CONFIG_CONNECT_CERTIFICATE_BASE64_KEY);

    if (key_base64 == NULL || cert_base64 == NULL) {
        _Z_ERROR("mTLS requires both client private key and certificate (base64)");
        return _Z_ERR_GENERIC;
    }

    z_result_t res = _z_tls_parse_key_from_base64(&ctx->_client_key, key_base64,
                                                  "client private key", &ctx->_hmac_drbg);
    if (res != _Z_RES_OK) {
        return res;
    }

    res = _z_tls_parse_cert_from_base64(&ctx->_client_cert, cert_base64, "client certificate");
    return res;
}

/* ============================================================================
 * TLS context management (static pool)
 * ============================================================================ */

_z_tls_context_t* _z_tls_context_new(void) {
    /* Find a free slot in the static pool */
    int slot = -1;
    for (int i = 0; i < ZPICO_SMOLTCP_MAX_TLS_SOCKETS; i++) {
        if (!TLS_CONTEXT_ALLOCATED[i]) {
            slot = i;
            break;
        }
    }
    if (slot < 0) {
        _Z_ERROR("TLS context pool exhausted (%d slots)", ZPICO_SMOLTCP_MAX_TLS_SOCKETS);
        return NULL;
    }

    _z_tls_context_t* ctx = &TLS_CONTEXT_POOL[slot];
    TLS_CONTEXT_ALLOCATED[slot] = true;

    mbedtls_ssl_init(&ctx->_ssl);
    mbedtls_ssl_config_init(&ctx->_ssl_config);
    mbedtls_entropy_init(&ctx->_entropy);
    mbedtls_hmac_drbg_init(&ctx->_hmac_drbg);
    mbedtls_x509_crt_init(&ctx->_ca_cert);
    mbedtls_pk_init(&ctx->_listen_key);
    mbedtls_x509_crt_init(&ctx->_listen_cert);
    mbedtls_pk_init(&ctx->_client_key);
    mbedtls_x509_crt_init(&ctx->_client_cert);
    ctx->_enable_mtls = false;

    int ret = mbedtls_hmac_drbg_seed(&ctx->_hmac_drbg, mbedtls_md_info_from_type(MBEDTLS_MD_SHA256),
                                     mbedtls_entropy_func, &ctx->_entropy, NULL, 0);
    if (ret != 0) {
        _Z_ERROR("Failed to seed HMAC_DRBG: -0x%04x", -ret);
        _z_tls_context_free(&ctx);
        return NULL;
    }

    return ctx;
}

void _z_tls_context_free(_z_tls_context_t** ctx) {
    if (ctx == NULL || *ctx == NULL) {
        return;
    }

    _z_tls_context_t* c = *ctx;

    mbedtls_ssl_free(&c->_ssl);
    mbedtls_ssl_config_free(&c->_ssl_config);
    mbedtls_entropy_free(&c->_entropy);
    mbedtls_hmac_drbg_free(&c->_hmac_drbg);
    mbedtls_x509_crt_free(&c->_ca_cert);
    mbedtls_pk_free(&c->_listen_key);
    mbedtls_x509_crt_free(&c->_listen_cert);
    mbedtls_pk_free(&c->_client_key);
    mbedtls_x509_crt_free(&c->_client_cert);

    /* Return slot to pool */
    for (int i = 0; i < ZPICO_SMOLTCP_MAX_TLS_SOCKETS; i++) {
        if (&TLS_CONTEXT_POOL[i] == c) {
            TLS_CONTEXT_ALLOCATED[i] = false;
            break;
        }
    }

    *ctx = NULL;
}

/* ============================================================================
 * TLS socket operations (the 9 platform symbols)
 * ============================================================================ */

z_result_t _z_open_tls(_z_tls_socket_t* sock, const _z_sys_net_endpoint_t* rep,
                       const char* hostname, const _z_str_intmap_t* config, bool peer_socket) {
    if (rep == NULL) {
        return _Z_ERR_GENERIC;
    }

    sock->_is_peer_socket = peer_socket;

    bool verify_name = true;
    const char* verify_opt = _z_str_intmap_get(config, TLS_CONFIG_VERIFY_NAME_ON_CONNECT_KEY);
    if (verify_opt != NULL && !_z_opt_is_true(verify_opt)) {
        verify_name = false;
    }

    bool enable_mtls = false;
    const char* mtls_opt = _z_str_intmap_get(config, TLS_CONFIG_ENABLE_MTLS_KEY);
    if (mtls_opt != NULL && _z_opt_is_true(mtls_opt)) {
        enable_mtls = true;
    }

    sock->_tls_ctx = _z_tls_context_new();
    if (sock->_tls_ctx == NULL) {
        return _Z_ERR_SYSTEM_OUT_OF_MEMORY;
    }

    if (enable_mtls) {
        z_result_t ret_client = _z_tls_load_client_cert(sock->_tls_ctx, config);
        if (ret_client != _Z_RES_OK) {
            _z_tls_context_free(&sock->_tls_ctx);
            return ret_client;
        }
    }
    sock->_tls_ctx->_enable_mtls = enable_mtls;

    z_result_t ret = _z_tls_load_ca_certificate(sock->_tls_ctx, config);
    if (ret != _Z_RES_OK) {
        _z_tls_context_free(&sock->_tls_ctx);
        return ret;
    }

    /* Open underlying TCP connection */
    ret = _z_open_tcp(&sock->_sock, *rep, Z_CONFIG_SOCKET_TIMEOUT);
    if (ret != _Z_RES_OK) {
        _z_tls_context_free(&sock->_tls_ctx);
        return ret;
    }

    /* Set back-pointer from TCP socket to TLS socket */
    sock->_sock._tls_sock = (void*)sock;

    /* Configure mbedTLS */
    int mbedret =
        mbedtls_ssl_config_defaults(&sock->_tls_ctx->_ssl_config, MBEDTLS_SSL_IS_CLIENT,
                                    MBEDTLS_SSL_TRANSPORT_STREAM, MBEDTLS_SSL_PRESET_DEFAULT);
    if (mbedret != 0) {
        _z_close_tcp(&sock->_sock);
        _z_tls_context_free(&sock->_tls_ctx);
        return _Z_ERR_GENERIC;
    }

    if (sock->_tls_ctx->_ca_cert.version != 0) {
        mbedtls_ssl_conf_ca_chain(&sock->_tls_ctx->_ssl_config, &sock->_tls_ctx->_ca_cert, NULL);
    }
    mbedtls_ssl_conf_authmode(&sock->_tls_ctx->_ssl_config, verify_name
                                                                ? MBEDTLS_SSL_VERIFY_REQUIRED
                                                                : MBEDTLS_SSL_VERIFY_OPTIONAL);
    mbedtls_ssl_conf_rng(&sock->_tls_ctx->_ssl_config, mbedtls_hmac_drbg_random,
                         &sock->_tls_ctx->_hmac_drbg);

    if (enable_mtls) {
        int own_ret =
            mbedtls_ssl_conf_own_cert(&sock->_tls_ctx->_ssl_config, &sock->_tls_ctx->_client_cert,
                                      &sock->_tls_ctx->_client_key);
        if (own_ret != 0) {
            _z_close_tcp(&sock->_sock);
            _z_tls_context_free(&sock->_tls_ctx);
            return _Z_ERR_GENERIC;
        }
    }

    mbedret = mbedtls_ssl_setup(&sock->_tls_ctx->_ssl, &sock->_tls_ctx->_ssl_config);
    if (mbedret != 0) {
        _z_close_tcp(&sock->_sock);
        _z_tls_context_free(&sock->_tls_ctx);
        return _Z_ERR_GENERIC;
    }

    if (hostname != NULL) {
        mbedret = mbedtls_ssl_set_hostname(&sock->_tls_ctx->_ssl, hostname);
        if (mbedret != 0) {
            _z_close_tcp(&sock->_sock);
            _z_tls_context_free(&sock->_tls_ctx);
            return _Z_ERR_GENERIC;
        }
    }

    /* Set BIO callbacks using the TCP socket handle for smoltcp I/O */
    mbedtls_ssl_set_bio(&sock->_tls_ctx->_ssl, &sock->_sock._handle, _z_tls_bio_send_smoltcp,
                        _z_tls_bio_recv_smoltcp, NULL);

    /* TLS handshake with smoltcp polling.
     * Unlike POSIX where the OS handles TCP I/O in the background,
     * smoltcp is cooperative — we must poll between WANT_READ/WANT_WRITE. */
    uint64_t handshake_start = smoltcp_clock_ms();
    while ((mbedret = mbedtls_ssl_handshake(&sock->_tls_ctx->_ssl)) != 0) {
        if (mbedret == MBEDTLS_ERR_SSL_WANT_READ || mbedret == MBEDTLS_ERR_SSL_WANT_WRITE) {
            smoltcp_poll_network();
            /* Check handshake timeout */
            if (smoltcp_clock_ms() - handshake_start > TLS_HANDSHAKE_TIMEOUT_MS) {
                _Z_ERROR("TLS handshake timed out");
                _z_close_tcp(&sock->_sock);
                _z_tls_context_free(&sock->_tls_ctx);
                return _Z_ERR_GENERIC;
            }
            continue;
        }
        _Z_ERROR("TLS handshake failed: -0x%04x", -mbedret);
        _z_close_tcp(&sock->_sock);
        _z_tls_context_free(&sock->_tls_ctx);
        return _Z_ERR_GENERIC;
    }

    /* Verify server certificate */
    uint32_t ignored_flags = verify_name ? 0u : MBEDTLS_X509_BADCERT_CN_MISMATCH;
    uint32_t verify_result = mbedtls_ssl_get_verify_result(&sock->_tls_ctx->_ssl);
    if (verify_result != 0) {
        if ((verify_result & ~ignored_flags) != 0u) {
            _Z_ERROR("TLS certificate verification failed: 0x%08x", (unsigned)verify_result);
            _z_close_tcp(&sock->_sock);
            _z_tls_context_free(&sock->_tls_ctx);
            return _Z_ERR_GENERIC;
        }
    }

    return _Z_RES_OK;
}

z_result_t _z_listen_tls(_z_tls_socket_t* sock, const char* host, const char* port,
                         const _z_str_intmap_t* config) {
    (void)sock;
    (void)host;
    (void)port;
    (void)config;
    /* Server-side TLS not supported on bare-metal (client-only) */
    return _Z_ERR_GENERIC;
}

z_result_t _z_tls_accept(_z_sys_net_socket_t* socket, const _z_sys_net_socket_t* listen_sock) {
    (void)socket;
    (void)listen_sock;
    /* Server-side TLS not supported on bare-metal (client-only) */
    return _Z_ERR_GENERIC;
}

void _z_close_tls(_z_tls_socket_t* sock) {
    if (sock->_tls_ctx != NULL) {
        mbedtls_ssl_close_notify(&sock->_tls_ctx->_ssl);
        _z_tls_context_free(&sock->_tls_ctx);
    }
    _z_close_tcp(&sock->_sock);
    sock->_sock._tls_sock = NULL;
}

size_t _z_read_tls(const _z_tls_socket_t* sock, uint8_t* ptr, size_t len) {
    if (sock->_tls_ctx == NULL) {
        return SIZE_MAX;
    }

    /* Poll network before read attempt */
    smoltcp_poll_network();

    int ret = mbedtls_ssl_read(&sock->_tls_ctx->_ssl, ptr, len);
    if (ret > 0) {
        return (size_t)ret;
    }

    if (ret == MBEDTLS_ERR_SSL_WANT_READ || ret == MBEDTLS_ERR_SSL_WANT_WRITE) {
        return 0;
    }

    if (ret == 0 || ret == MBEDTLS_ERR_SSL_PEER_CLOSE_NOTIFY || ret == MBEDTLS_ERR_SSL_CONN_EOF) {
        return SIZE_MAX;
    }

    return SIZE_MAX;
}

size_t _z_write_tls(const _z_tls_socket_t* sock, const uint8_t* ptr, size_t len) {
    if (sock->_tls_ctx == NULL) {
        return SIZE_MAX;
    }

    /* Poll network before write attempt */
    smoltcp_poll_network();

    int ret = mbedtls_ssl_write(&sock->_tls_ctx->_ssl, ptr, len);
    if (ret > 0) {
        return (size_t)ret;
    }

    if (ret == MBEDTLS_ERR_SSL_WANT_READ || ret == MBEDTLS_ERR_SSL_WANT_WRITE) {
        return 0;
    }

    return SIZE_MAX;
}

size_t _z_write_all_tls(const _z_tls_socket_t* sock, const uint8_t* ptr, size_t len) {
    size_t n = 0;
    do {
        size_t wb = _z_write_tls(sock, &ptr[n], len - n);
        if (wb == SIZE_MAX) {
            return wb;
        }
        n += wb;
    } while (n < len);
    return n;
}

#endif /* Z_FEATURE_LINK_TLS == 1 */
