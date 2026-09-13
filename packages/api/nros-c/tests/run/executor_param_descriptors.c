/* phase-417 W4.a — the C descriptor, undeclare, listing and on-set surface
 * reaches the ONE store, and its rules are the store's rules.
 *
 * Until this wave the C surface had no descriptor verbs at all, so
 * `~/describe_parameters` answered with an EMPTY descriptor for everything a C
 * image declared, `ros2 param set` could not be refused on a range a C caller
 * meant to impose, and the accept/reject callback C did ship
 * (`nros_parameter_server_set_callback`) sat on the legacy store that no
 * service reads — it fired for nobody (issue 0793).
 *
 * COMPILE-AND-RUN rather than a signature probe, for the reason
 * `executor_param_node_keying.c` gives one file over: the defect is a
 * BEHAVIOUR. A `nros_executor_add_param_constraint_double` that stored the
 * range in a table of its own, or forwarded to the legacy store, would
 * compile, link and pass every declaration check, while the assertions below
 * about a refused write would fail. That is RFC-0089's "compiles and differs",
 * and only a run catches it.
 *
 * The RMW backend is the shared stub (`stub_rmw_backend.c`): nothing on the
 * parameter path touches the wire, so this stays a source-gate test with no
 * router, no agent and no timing, while still driving the real `Executor`, the
 * real `nros_params::ParameterServer` and the real `apply` rules.
 *
 * The lane builds this probe's `libnros_c.a` with
 * `NROS_MAX_PARAM_CONSTRAINTS_LEN=256`. That capacity DEFAULTS TO 0 -- an
 * image pays for `additional_constraints` text only if it asks for it -- so a
 * probe built at the default would read every constraints round-trip below as
 * an empty string and pass while proving nothing.
 */

#include <nros/nros.h>
#include <nros/rmw_vtable.h>

#include <stdio.h>
#include <string.h>

#include "stub_rmw_backend.h"

static int g_failures;

#define CHECK(cond, ...)                                                                           \
    do {                                                                                           \
        if (!(cond)) {                                                                             \
            g_failures++;                                                                          \
            printf("FAIL %s:%d: ", __FILE__, __LINE__);                                            \
            printf(__VA_ARGS__);                                                                   \
            printf("\n");                                                                          \
        }                                                                                          \
    } while (0)

/* The C path has no weak default, so the TU that links the stub owns this. */
void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

/* The hook under test. It refuses exactly one value, so a run that never
 * reaches it and a run that always refuses are both visibly wrong. */
static int g_hook_calls;

static bool refuse_thirteen(const char* name, const struct nros_parameter_t* param, void* context) {
    (void)name;
    g_hook_calls++;
    *(int*)context += 1;
    if (param->type == NROS_PARAMETER_INTEGER && param->value.integer_value == 13) {
        return false;
    }
    return true;
}

int main(void) {
    struct nros_support_t support;
    struct nros_executor_t executor = rclc_executor_get_zero_initialized_executor();
    struct nros_node_t talker;
    char description[64];
    char constraints[64];
    char names[8][32];
    bool read_only = true;
    enum nros_parameter_type_t type = NROS_PARAMETER_NOT_SET;
    int64_t from_i = 0, to_i = 0, step_i = 0;
    double from_d = 0.0, to_d = 0.0, step_d = 0.0;
    size_t count = 0;
    int64_t got = 0;
    double gotd = 0.0;
    int hook_context = 0;
    uint16_t handle = 0xFFFF;

    memset(&support, 0, sizeof(support));
    memset(&talker, 0, sizeof(talker));

    CHECK(nros_support_init_rmw(&support, NULL, 0, "param_descriptors", NROS_STUB_RMW_NAME) ==
              NROS_RET_OK,
          "support init");
    CHECK(nros_executor_init(&executor, &support, 8) == NROS_RET_OK, "executor init");
    CHECK(nros_executor_node_init(&executor, &talker, "talker", NULL) == NROS_RET_OK,
          "talker node init");
    if (g_failures != 0) {
        printf("executor_param_descriptors: %d setup failure(s)\n", g_failures);
        return 1;
    }

    /* ---- descriptors attach AFTER the declaration, as rclc does ---------- */

    CHECK(nros_executor_declare_param_double_on(&executor, &talker, "rate", 2.0) == NROS_RET_OK,
          "declare rate");
    CHECK(nros_executor_add_param_description_on(&executor, &talker, "rate", "publish rate",
                                                 "hz, positive") == NROS_RET_OK,
          "attach a description to a declared parameter");
    CHECK(nros_executor_add_param_constraint_double_on(&executor, &talker, "rate", 0.0, 10.0,
                                                       0.0) == NROS_RET_OK,
          "attach a double range");

    CHECK(nros_executor_describe_param_on(&executor, &talker, "rate", description,
                                          sizeof(description), constraints, sizeof(constraints),
                                          &read_only, &type) == NROS_RET_OK,
          "describe rate");
    CHECK(strcmp(description, "publish rate") == 0, "description read back as \"%s\"", description);
    CHECK(strcmp(constraints, "hz, positive") == 0, "constraints read back as \"%s\"", constraints);
    CHECK(read_only == false, "a parameter nobody marked read-only came back read-only");
    CHECK(type == NROS_PARAMETER_DOUBLE, "describe reported type %d", (int)type);
    CHECK(nros_executor_get_param_type_on(&executor, &talker, "rate") == NROS_PARAMETER_DOUBLE,
          "the type query disagrees with describe");
    CHECK(nros_executor_get_param_type_on(&executor, &talker, "absent") == NROS_PARAMETER_NOT_SET,
          "an undeclared name answered with a type");

    CHECK(nros_executor_get_param_double_range_on(&executor, &talker, "rate", &from_d, &to_d,
                                                  &step_d) == NROS_RET_OK,
          "read the double range back");
    CHECK(from_d == 0.0 && to_d == 10.0, "range read back as [%f, %f]", from_d, to_d);
    CHECK(nros_executor_get_param_integer_range_on(&executor, &talker, "rate", &from_i, &to_i,
                                                   &step_i) == NROS_RET_NOT_FOUND,
          "a double parameter answered with an INTEGER range");

    /* THE acceptance for ranges: the constraint is ENFORCED by the same
     * `apply` a remote `ros2 param set` reaches, not merely recorded. A
     * wrapper that kept the range in a table of its own would pass every
     * assertion above and fail this one. */
    CHECK(nros_executor_set_param_double_on(&executor, &talker, "rate", 50.0) ==
              NROS_RET_INVALID_ARGUMENT,
          "a write outside the attached range was accepted");
    CHECK(nros_executor_get_param_double_on(&executor, &talker, "rate", &gotd) == NROS_RET_OK &&
              gotd == 2.0,
          "a refused write moved the value to %f", gotd);
    CHECK(nros_executor_set_param_double_on(&executor, &talker, "rate", 5.0) == NROS_RET_OK,
          "a write INSIDE the range was refused");

    /* An ill-formed range, and one the current value already breaks, are both
     * refused: attaching either would advertise a bound no set could produce. */
    CHECK(nros_executor_declare_param_integer_on(&executor, &talker, "depth", 100) == NROS_RET_OK,
          "declare depth");
    CHECK(nros_executor_add_param_constraint_integer_on(&executor, &talker, "depth", 10, 0, 1) ==
              NROS_RET_INVALID_ARGUMENT,
          "an inverted range was accepted");
    CHECK(nros_executor_add_param_constraint_integer_on(&executor, &talker, "depth", 0, 10, 1) ==
              NROS_RET_INVALID_ARGUMENT,
          "a range the CURRENT value breaks was accepted");
    CHECK(nros_executor_add_param_constraint_integer_on(&executor, &talker, "depth", 0, 1000, 1) ==
              NROS_RET_OK,
          "a well-formed range the value satisfies was refused");

    /* read-only, the same way */
    CHECK(nros_executor_set_param_read_only_on(&executor, &talker, "depth", true) == NROS_RET_OK,
          "mark depth read-only");
    CHECK(nros_executor_set_param_integer_on(&executor, &talker, "depth", 7) ==
              NROS_RET_INVALID_ARGUMENT,
          "a read-only parameter accepted a write");
    CHECK(nros_executor_describe_param_on(&executor, &talker, "depth", NULL, 0, NULL, 0, &read_only,
                                          NULL) == NROS_RET_OK &&
              read_only,
          "describe did not report depth read-only");

    /* Every out-param is optional, and an undeclared name is NOT_FOUND. */
    CHECK(nros_executor_describe_param_on(&executor, &talker, "absent", NULL, 0, NULL, 0, NULL,
                                          NULL) == NROS_RET_NOT_FOUND,
          "describe answered for a parameter nobody declared");
    /* A short text buffer TRUNCATES and says so, rather than refusing: a
     * prefix of a description is still usable. */
    CHECK(nros_executor_describe_param_on(&executor, &talker, "rate", description, 5, NULL, 0, NULL,
                                          NULL) == NROS_RET_FULL,
          "a short description buffer did not report NROS_RET_FULL");
    CHECK(strcmp(description, "publ") == 0, "truncated description is \"%s\"", description);

    /* ---- listing -------------------------------------------------------- */

    CHECK(nros_executor_list_params_on(&executor, &talker, NULL, NULL, 0, 0, &count) ==
                  NROS_RET_FULL &&
              count == 2,
          "counting pass reported %u names, expected 2", (unsigned)count);
    CHECK(nros_executor_list_params_on(&executor, &talker, NULL, &names[0][0], sizeof(names[0]), 8,
                                       &count) == NROS_RET_OK &&
              count == 2,
          "listing pass reported %u names", (unsigned)count);
    CHECK((strcmp(names[0], "rate") == 0 && strcmp(names[1], "depth") == 0) ||
              (strcmp(names[0], "depth") == 0 && strcmp(names[1], "rate") == 0),
          "listed \"%s\" and \"%s\"", names[0], names[1]);
    CHECK(nros_executor_list_params_on(&executor, &talker, "ra", &names[0][0], sizeof(names[0]), 8,
                                       &count) == NROS_RET_OK &&
              count == 1 && strcmp(names[0], "rate") == 0,
          "prefix filter returned %u names, first \"%s\"", (unsigned)count, names[0]);

    /* ---- undeclare ------------------------------------------------------ */

    CHECK(nros_executor_delete_param_on(&executor, &talker, "depth") == NROS_RET_OK,
          "undeclare depth");
    CHECK(!nros_executor_has_param_on(&executor, &talker, "depth"),
          "an undeclared parameter is still there");
    CHECK(nros_executor_delete_param_on(&executor, &talker, "depth") == NROS_RET_NOT_FOUND,
          "undeclaring twice must be NOT_FOUND, not OK");
    /* The slot is FREE, and the name is free WITH IT — including its
     * descriptor, so a re-declared parameter does not inherit the old
     * read-only flag. */
    CHECK(nros_executor_declare_param_integer_on(&executor, &talker, "depth", 3) == NROS_RET_OK,
          "re-declare after undeclare");
    CHECK(nros_executor_set_param_integer_on(&executor, &talker, "depth", 4) == NROS_RET_OK,
          "a re-declared parameter inherited the old read-only flag");

    /* ---- the accept/reject hook, on the store the services read --------- */

    CHECK(nros_executor_set_param_callback_on(&executor, &talker, refuse_thirteen, &hook_context,
                                              &handle) == NROS_RET_OK,
          "register the on-set hook");
    CHECK(nros_executor_set_param_integer_on(&executor, &talker, "depth", 5) == NROS_RET_OK,
          "the hook refused a value it should have passed");
    CHECK(g_hook_calls == 1, "the hook ran %d times for one write", g_hook_calls);
    CHECK(hook_context == 1, "the context did not reach the hook (%d)", hook_context);

    CHECK(nros_executor_set_param_integer_on(&executor, &talker, "depth", 13) ==
              NROS_RET_INVALID_ARGUMENT,
          "the hook's refusal did not reach the caller");
    CHECK(nros_executor_get_param_integer_on(&executor, &talker, "depth", &got) == NROS_RET_OK &&
              got == 5,
          "a hook-refused write still moved the value to %lld", (long long)got);

    /* The hook runs AFTER the store's rules, so a write read-only already
     * refused never reaches it. `rate` is still range-constrained above. */
    g_hook_calls = 0;
    CHECK(nros_executor_set_param_double_on(&executor, &talker, "rate", 99.0) ==
              NROS_RET_INVALID_ARGUMENT,
          "the range stopped applying once a hook was registered");
    CHECK(g_hook_calls == 0, "the hook saw a write the range had already refused");

    CHECK(nros_executor_remove_param_callback(&executor, handle) == NROS_RET_OK,
          "unregister the hook");
    CHECK(nros_executor_remove_param_callback(&executor, handle) == NROS_RET_NOT_FOUND,
          "unregistering twice must be NOT_FOUND");
    CHECK(nros_executor_set_param_integer_on(&executor, &talker, "depth", 13) == NROS_RET_OK,
          "an unregistered hook is still refusing");

    /* A NULL callback is a programming error, not a way to clear: the handle
     * is what clears. */
    CHECK(nros_executor_set_param_callback_on(&executor, &talker, NULL, NULL, NULL) ==
              NROS_RET_INVALID_ARGUMENT,
          "a NULL callback was accepted as a registration");

    if (g_failures != 0) {
        printf("executor_param_descriptors: %d failure(s)\n", g_failures);
        return 1;
    }
    printf("executor_param_descriptors: descriptors, listing, undeclare and the on-set hook "
           "all reach the one store\n");
    return 0;
}
