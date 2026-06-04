# Test message for parser edge cases using test_msgs types
# Tests: bounded sequences, bounded strings, nested types, constants, defaults

# Bounded types from test_msgs
test_msgs/BoundedPlainSequences bounded_primitives
test_msgs/BoundedSequences bounded_sequences
test_msgs/Strings string_types
test_msgs/WStrings wide_strings

# Deep nesting
test_msgs/MultiNested deeply_nested

# Basic types with all primitives
test_msgs/BasicTypes all_primitives

# Constants and defaults
test_msgs/Constants constant_values
test_msgs/Defaults default_values

# Arrays of different sizes
test_msgs/Arrays array_types

# Empty message edge case
test_msgs/Empty empty_msg

# Unbounded sequences
test_msgs/UnboundedSequences unbounded_seqs
