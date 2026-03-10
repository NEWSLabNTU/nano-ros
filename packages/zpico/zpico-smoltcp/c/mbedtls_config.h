/**
 * Minimal mbedTLS configuration for bare-metal TLS client.
 *
 * Enables: TLS 1.2 client, X.509 cert parsing, SHA-256, AES-GCM/CBC,
 *          RSA, ECDHE (secp256r1), HMAC_DRBG.
 * Disables: server, DTLS, filesystem, threading, POSIX sockets.
 *
 * Used via -DMBEDTLS_CONFIG_FILE="mbedtls_config.h" to replace the
 * default mbedtls/config.h.
 */

#ifndef MBEDTLS_BARE_METAL_CONFIG_H
#define MBEDTLS_BARE_METAL_CONFIG_H

/* ============================================================================
 * System support
 * ============================================================================ */

/* Custom allocator — use z_malloc/z_free from platform crate.
 * MBEDTLS_PLATFORM_MEMORY + MBEDTLS_PLATFORM_{CALLOC,FREE}_MACRO
 * replaces mbedtls_calloc/mbedtls_free with compile-time macros,
 * completely eliminating references to stdlib calloc/free. */
#define MBEDTLS_PLATFORM_C
#define MBEDTLS_PLATFORM_MEMORY

/* Bare-metal allocator functions (implemented in tls_bare_metal.c) */
#include <stddef.h>
void* z_bare_metal_calloc(size_t n, size_t size);
void z_bare_metal_free(void* ptr);
#define MBEDTLS_PLATFORM_CALLOC_MACRO z_bare_metal_calloc
#define MBEDTLS_PLATFORM_FREE_MACRO z_bare_metal_free

/* No system time (bare-metal has no RTC) */
/* #undef MBEDTLS_HAVE_TIME */
/* #undef MBEDTLS_HAVE_TIME_DATE */

/* No filesystem */
/* #undef MBEDTLS_FS_IO */

/* No POSIX sockets (we use smoltcp BIO callbacks) */
/* #undef MBEDTLS_NET_C */

/* No threading */
/* #undef MBEDTLS_THREADING_C */
/* #undef MBEDTLS_THREADING_PTHREAD */

/* No NV seed (no filesystem for persistent entropy seed) */
/* #undef MBEDTLS_ENTROPY_NV_SEED */

/* Custom entropy — platform provides mbedtls_hardware_poll() */
#define MBEDTLS_NO_PLATFORM_ENTROPY
#define MBEDTLS_ENTROPY_HARDWARE_ALT

/* ============================================================================
 * TLS protocol
 * ============================================================================ */

#define MBEDTLS_SSL_CLI_C
/* #undef MBEDTLS_SSL_SRV_C */ /* client only */
/* #undef MBEDTLS_SSL_DTLS_HELLO_VERIFY */
/* #undef MBEDTLS_SSL_PROTO_DTLS */

#define MBEDTLS_SSL_TLS_C
#define MBEDTLS_SSL_PROTO_TLS1_2

/* ============================================================================
 * X.509 certificate parsing
 * ============================================================================ */

#define MBEDTLS_X509_USE_C
#define MBEDTLS_X509_CRT_PARSE_C
/* #undef MBEDTLS_X509_CRL_PARSE_C */
/* #undef MBEDTLS_X509_CSR_PARSE_C */
/* #undef MBEDTLS_X509_CREATE_C */
/* #undef MBEDTLS_X509_CRT_WRITE_C */
/* #undef MBEDTLS_X509_CSR_WRITE_C */

#define MBEDTLS_ASN1_PARSE_C
#define MBEDTLS_ASN1_WRITE_C
#define MBEDTLS_BASE64_C
#define MBEDTLS_OID_C
#define MBEDTLS_PEM_PARSE_C

/* ============================================================================
 * Cryptographic primitives
 * ============================================================================ */

/* Hash */
#define MBEDTLS_MD_C
#define MBEDTLS_SHA256_C
#define MBEDTLS_SHA224_C
#define MBEDTLS_SHA512_C /* needed for some certificate chains */

/* Symmetric ciphers */
#define MBEDTLS_CIPHER_C
#define MBEDTLS_AES_C
#define MBEDTLS_GCM_C
#define MBEDTLS_CIPHER_MODE_CBC

/* Big number math */
#define MBEDTLS_BIGNUM_C

/* RSA */
#define MBEDTLS_RSA_C
#define MBEDTLS_PKCS1_V15
#define MBEDTLS_PKCS1_V21

/* Elliptic curves */
#define MBEDTLS_ECP_C
#define MBEDTLS_ECDH_C
#define MBEDTLS_ECDSA_C
#define MBEDTLS_ECP_DP_SECP256R1_ENABLED
#define MBEDTLS_ECP_DP_SECP384R1_ENABLED

/* Public key abstraction */
#define MBEDTLS_PK_C
#define MBEDTLS_PK_PARSE_C

/* ============================================================================
 * Key exchange and ciphersuites
 * ============================================================================ */

#define MBEDTLS_KEY_EXCHANGE_ECDHE_RSA_ENABLED
#define MBEDTLS_KEY_EXCHANGE_ECDHE_ECDSA_ENABLED
#define MBEDTLS_KEY_EXCHANGE_RSA_ENABLED

/* ============================================================================
 * Random number generation
 * ============================================================================ */

#define MBEDTLS_ENTROPY_C
#define MBEDTLS_HMAC_DRBG_C
#define MBEDTLS_CTR_DRBG_C

/* ============================================================================
 * Tuning for memory-constrained systems
 * ============================================================================ */

/* Reduce peak memory for MPI operations */
#define MBEDTLS_MPI_MAX_SIZE 512
#define MBEDTLS_MPI_WINDOW_SIZE 2

/* Reduce SSL buffer sizes — zenoh frames are typically small */
#define MBEDTLS_SSL_MAX_CONTENT_LEN 4096
#define MBEDTLS_SSL_IN_CONTENT_LEN 4096
#define MBEDTLS_SSL_OUT_CONTENT_LEN 4096

/* ============================================================================
 * Include check_config.h to validate this configuration
 * ============================================================================ */

#include "mbedtls/check_config.h"

#endif /* MBEDTLS_BARE_METAL_CONFIG_H */
