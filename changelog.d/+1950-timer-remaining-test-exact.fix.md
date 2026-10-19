`time_until_next_call_goes_negative_once_a_timer_is_overdue` no longer fails
when the spin itself is slow: it runs on a private clock, so "60 ms remain"
and "50 ms overdue" are exact readings rather than ranges whose floor load
pushed through (issue 1950).
