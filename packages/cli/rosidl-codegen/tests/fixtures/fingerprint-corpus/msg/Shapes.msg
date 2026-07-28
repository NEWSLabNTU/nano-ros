# Every field shape whose emitted code differs. Keep in sync with the emitter
# branches in generator/common.rs (primitive / string / array / sequence /
# nested / bounded), not with any real message.
bool     flag
int8     i8_v
uint8    u8_v
int16    i16_v
uint16   u16_v
int32    i32_v
uint32   u32_v
int64    i64_v
uint64   u64_v
float32  f32_v
float64  f64_v
string   text
int64[]      seq_prim
string[]     seq_string
float64[3]   arr_fixed
int32[<=4]   seq_bounded
string<=8    str_bounded
