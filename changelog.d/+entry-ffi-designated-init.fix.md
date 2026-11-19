Generated C entries now write every FFI table row with designated initialisers,
and generated C++ entries check the positional rows they write against the
struct layout at compile time, so a field inserted into one of those structs
can no longer silently shift the values after it.
