/*
 * phase-515 C5 -- rcl's error state, at rcl's names:
 *
 *     if (rclc_executor_spin_some(NULL, 0) != RCL_RET_OK) {
 *         printf("%s\n", rcl_get_error_string().str);   // "executor argument is null, at ..."
 *         rcl_reset_error();
 *     }
 *
 * Asserted here:
 *   1. nothing is set at start: `rcl_error_is_set` false, `rcl_get_error_state`
 *      NULL, the string rcutils' "error not set";
 *   2. a refused argument inside the C API records its NAME, file and line;
 *   3. `RCL_SET_ERROR_MSG` and `RCL_CHECK_ARGUMENT_FOR_NULL` record the caller's;
 *   4. a message longer than the bounded buffer is truncated and terminated;
 *   5. `rcl_reset_error` forgets it.
 */

#include "stub_rmw_backend.h"

#include <rcl/error_handling.h>

#include <stdbool.h>
#include <stdio.h>
#include <string.h>

void nros_app_register_backends(void);
void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

static int s_failures = 0;

#define CHECK(cond, msg)                                                                           \
    do {                                                                                           \
        if (!(cond)) {                                                                             \
            fprintf(stderr, "FAIL: %s (%s:%d)\n", (msg), __FILE__, __LINE__);                      \
            s_failures++;                                                                          \
        }                                                                                          \
    } while (0)

static rcl_ret_t needs_pointer(const int* p) {
    RCL_CHECK_ARGUMENT_FOR_NULL(p, RCL_RET_INVALID_ARGUMENT);
    return RCL_RET_OK;
}

int main(void) {
    /* rcl's signatures, taken as pointers so a drift fails to compile. */
    rcl_error_string_t (*get_string)(void) = rcl_get_error_string;
    const rcl_error_state_t* (*get_state)(void) = rcl_get_error_state;
    bool (*is_set)(void) = rcl_error_is_set;
    void (*reset)(void) = rcl_reset_error;
    void (*set_state)(const char*, const char*, size_t) = rcl_set_error_state;
    (void)set_state;

    /* 1 */
    CHECK(!is_set(), "nothing set at start");
    CHECK(get_state() == NULL, "no state at start");
    CHECK(strcmp(get_string().str, "error not set") == 0, "rcutils' unset text");

    /* 2 */
    CHECK(rclc_executor_spin_some(NULL, 0) == RCL_RET_INVALID_ARGUMENT, "NULL executor refused");
    CHECK(is_set(), "...and recorded");
    const rcl_error_state_t* st = get_state();
    CHECK(st != NULL && strcmp(st->message, "executor argument is null") == 0,
          "the message names the argument");
    CHECK(st != NULL && strstr(st->file, "executor.rs") != NULL && st->line_number > 0,
          "the site's file and line");
    rcl_error_string_t s = get_string();
    CHECK(strncmp(s.str, "executor argument is null, at ", 30) == 0, "rcutils' format");
    CHECK(strchr(s.str, ':') != NULL, "...with the line after a colon");
    printf("recorded: %s\n", s.str);

    /* 3 */
    reset();
    RCL_SET_ERROR_MSG("caller said so");
    CHECK(strncmp(get_string().str, "caller said so, at ", 19) == 0, "RCL_SET_ERROR_MSG");
    CHECK(get_state()->line_number == (uint64_t)(__LINE__ - 2), "...with the caller's line");
    CHECK(needs_pointer(NULL) == RCL_RET_INVALID_ARGUMENT, "RCL_CHECK_ARGUMENT_FOR_NULL returns");
    CHECK(strcmp(get_state()->message, "p argument is null") == 0, "...and names the argument");

    /* 4 */
    char longmsg[600];
    memset(longmsg, 'x', sizeof longmsg - 1);
    longmsg[sizeof longmsg - 1] = '\0';
    RCL_SET_ERROR_MSG(longmsg);
    CHECK(strlen(get_state()->message) == RCUTILS_ERROR_STATE_MESSAGE_MAX_LENGTH - 1,
          "a long message is truncated to the buffer");
    s = get_string();
    CHECK(strlen(s.str) < RCUTILS_ERROR_MESSAGE_MAX_LENGTH, "the string stays terminated");
    CHECK(sizeof(rcl_error_string_t) == RCUTILS_ERROR_MESSAGE_MAX_LENGTH, "256 bytes by value");

    /* 5 */
    reset();
    CHECK(!is_set() && get_state() == NULL, "reset forgets");
    CHECK(strcmp(get_string().str, "error not set") == 0, "...back to the unset text");

    if (s_failures != 0) {
        fprintf(stderr, "rcl_error_state: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("rcl_error_state: OK\n");
    return 0;
}
