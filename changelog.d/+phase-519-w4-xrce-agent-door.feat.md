`just xrce agent [<locator> | <leaf>[:<image>]]` starts the provisioned
Micro-XRCE-DDS Agent on a locator in nano-ros's own spelling (`udp/127.0.0.1:8888`,
`tcp/…`, `serial/<dev>#baudrate=<n>`), or on the locator a leaf's image states,
instead of a hand-typed `MicroXRCEAgent udp4 -p 8888`. The examples and the PX4
companion guide use it.
