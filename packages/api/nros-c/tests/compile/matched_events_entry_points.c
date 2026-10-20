/* Issue 1972 — the matched and incompatible-type setters must be DECLARED,
 * callable, and take callbacks of the shapes a C user will write.
 *
 * Function POINTERS and real callback definitions, not calls: a syntax-only
 * TU with no entity to pass. A missing declaration, a drifted signature, or a
 * payload struct missing a field fails to compile here rather than at a user's
 * link step. */

#include <rclc/rclc.h>
#include <stdint.h>

static void on_pub_matched(rcl_publisher_t* pub, rclc_matched_status_t status, void* ctx) {
    (void)pub;
    (void)ctx;
    /* All four of upstream's `rmw_matched_status_t` fields, signed change. */
    int32_t delta = status.current_count_change;
    uint32_t sum = status.total_count + status.total_count_change + status.current_count;
    (void)delta;
    (void)sum;
}

static void on_sub_matched(rcl_subscription_t* sub, rclc_matched_status_t status, void* ctx) {
    (void)sub;
    (void)status;
    (void)ctx;
}

static void on_pub_incompatible(rcl_publisher_t* pub, rclc_incompatible_type_status_t status,
                                void* ctx) {
    (void)pub;
    (void)ctx;
    uint32_t n = status.total_count + status.total_count_change;
    (void)n;
}

static void on_sub_incompatible(rcl_subscription_t* sub, rclc_incompatible_type_status_t status,
                                void* ctx) {
    (void)sub;
    (void)status;
    (void)ctx;
}

int main(void) {
    rcl_ret_t (*p_pub_matched)(rcl_publisher_t*, rclc_event_publisher_matched_cb_t, void*) =
        rclc_publisher_set_matched_callback;
    rcl_ret_t (*p_pub_incompat)(rcl_publisher_t*, rclc_event_publisher_incompatible_type_cb_t,
                                void*) = rclc_publisher_set_incompatible_type_callback;
    rcl_ret_t (*p_sub_matched)(rcl_subscription_t*, rclc_event_subscription_matched_cb_t, void*) =
        rclc_subscription_set_matched_callback;
    rcl_ret_t (*p_sub_incompat)(rcl_subscription_t*,
                                rclc_event_subscription_incompatible_type_cb_t, void*) =
        rclc_subscription_set_incompatible_type_callback;
    rclc_event_publisher_matched_cb_t a = on_pub_matched;
    rclc_event_subscription_matched_cb_t b = on_sub_matched;
    rclc_event_publisher_incompatible_type_cb_t c = on_pub_incompatible;
    rclc_event_subscription_incompatible_type_cb_t d = on_sub_incompatible;
    (void)p_pub_matched;
    (void)p_pub_incompat;
    (void)p_sub_matched;
    (void)p_sub_incompat;
    (void)a;
    (void)b;
    (void)c;
    (void)d;
    return 0;
}
