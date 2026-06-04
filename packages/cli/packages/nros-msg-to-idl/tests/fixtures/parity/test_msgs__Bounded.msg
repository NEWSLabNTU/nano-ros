# Exercises bounded scalar / bounded array / bounded string field shapes
# so the parity test covers the `<=` bound-token paths in the parser
# (the existing rcl-interfaces fixtures don't use any bounded forms).

string<=64 name
int8[<=16] payload
