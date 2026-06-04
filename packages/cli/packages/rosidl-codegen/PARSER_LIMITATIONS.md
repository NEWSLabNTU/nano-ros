# rosidl-parser Known Limitations

This document lists ROS 2 messages that currently fail to parse due to parser limitations.

## Summary

**Overall Success Rate**: 97.8% (87 out of 89 tested messages)

| Package | Total | Passing | Failing | Success Rate |
|---------|-------|---------|---------|--------------|
| std_msgs | 30 | 30 | 0 | 100% |
| geometry_msgs | 32 | 31 | 1 | 96.9% |
| sensor_msgs | 27 | 26 | 1 | 96.3% |
| **Total** | **89** | **87** | **2** | **97.8%** |

## Failing Messages

### 1. geometry_msgs/Quaternion.msg

**Reason**: Parser does not support default field values

**File Location**: `/opt/ros/jazzy/share/geometry_msgs/msg/Quaternion.msg`

**Content**:
```
# This represents an orientation in free space in quaternion form.

float64 x 0
float64 y 0
float64 z 0
float64 w 1
```

**Error**: `UnknownType("0")`

**Explanation**: The parser treats `0` and `1` as type names instead of recognizing them as default values for the float64 fields.

**Required Fix**: Update parser grammar to support default value syntax:
- `field_type field_name default_value`
- Store default value in Field AST node
- Update codegen to emit default values in Default::default()

### 2. sensor_msgs/NavSatStatus.msg

**Reason**: Parser does not support negative integer constants

**File Location**: `/opt/ros/jazzy/share/sensor_msgs/msg/NavSatStatus.msg`

**Relevant Content**:
```
int8 STATUS_UNKNOWN = -2        # status is not yet set
int8 STATUS_NO_FIX =  -1        # unable to fix position
int8 STATUS_FIX =      0        # unaugmented fix
...
int8 status -2 # STATUS_UNKNOWN
```

**Error**: `LexerError("Unexpected character at position 280: '-'")`

**Explanation**: The lexer does not recognize `-` as part of a numeric literal, failing to parse negative constants.

**Required Fix**: Update lexer to handle negative numbers:
- Accept `-` before numeric literals
- Parse as single token (negative number) not separate tokens
- Support for all integer types: int8, int16, int32, int64

## Tested Packages

The following packages were tested for parser compatibility:

### Fully Tested (with parse_all tests)
- ✅ **std_msgs** (30 messages): 100% success
- ✅ **geometry_msgs** (32 messages): 96.9% success
- ✅ **sensor_msgs** (27 messages): 96.3% success

### Spot Tested (individual messages)
- ✅ **example_interfaces** (services/actions): All tested messages pass
- ✅ **builtin_interfaces** (2 messages): All pass
- ✅ **action_msgs** (3 messages): All pass

### Not Yet Tested
- nav_msgs (6 messages)
- diagnostic_msgs (3 messages)
- lifecycle_msgs (4 messages)
- rosgraph_msgs (1 message)
- actuator_msgs
- composition_interfaces
- gps_msgs
- map_msgs
- pcl_msgs
- pendulum_msgs
- rcl_interfaces
- rosbag2_interfaces
- ... and others

## Workarounds

Until these limitations are fixed, users can:

1. **For Quaternion**: Manually specify default values in code instead of relying on generated defaults
2. **For NavSatStatus**: Use constant values instead of the STATUS_UNKNOWN name in initialization

## Roadmap

These limitations are addressed in:
- **Subphase 1.5: Parser Enhancements** (see ROADMAP.md)
  - Support for negative integer constants
  - Support for default field values
  - Target: 100% parsing success rate

## Testing

To check current parser status:

```bash
# Test individual packages
cargo test --test parity_test test_parse_all_std_msgs -- --nocapture
cargo test --test parity_test test_parse_all_geometry_msgs -- --nocapture
cargo test --test parity_test test_parse_all_sensor_msgs -- --nocapture

# Check specific failing messages (will currently fail)
rosidl-parser parse /opt/ros/jazzy/share/geometry_msgs/msg/Quaternion.msg
rosidl-parser parse /opt/ros/jazzy/share/sensor_msgs/msg/NavSatStatus.msg
```

## Last Updated

2025-01-29 (Phase 1.4 completion)
