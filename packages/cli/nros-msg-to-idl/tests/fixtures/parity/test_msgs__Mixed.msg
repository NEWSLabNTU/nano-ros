# Edge fixture -- union of the field-shape kinds covered by the parity
# corpus: primitive scalars, fixed-size array, unbounded sequence,
# bounded sequence, bounded string, nested-msg reference.
# (ASCII-only on purpose; see parity.rs `KNOWN_DIVERGENCE_NON_ASCII`.)

int32 scalar
uint8[4] fixed_quad
int8[] unbounded
int16[<=8] bounded
string<=32 label
geometry_msgs/Vector3 origin
