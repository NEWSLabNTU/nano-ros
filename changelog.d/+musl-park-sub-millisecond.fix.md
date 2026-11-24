On Linux with musl, the executor now waits with microsecond precision between timer deadlines, as it already did with glibc, instead of rounding every sub-millisecond wait up to a whole millisecond.
