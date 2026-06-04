# Robot operational status
uint8 STATUS_IDLE = 0
uint8 STATUS_MOVING = 1
uint8 STATUS_ERROR = 2

std_msgs/Header header
uint8 status
float64 battery_level
string mode
