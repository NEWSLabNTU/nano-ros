A C or C++ image whose launch declares string or array parameters must now state how long they may be, because it is now sized from its own declarations and no longer from a configure it shared with images that declare none. Add the lines to the image in `system.toml` (two tables, `params` for the parameter store and `param` for the parameter-service inbox):

```toml
[image.<id>.knobs.params]
max_string_value_bytes = 16   # the longest string value
max_array_len = 4             # the longest bool, integer or double array
max_string_array_len = 4      # the longest string array
max_byte_array_bytes = 256    # the longest byte array (only for a byte_array)

[image.<id>.knobs.param]
service_inbox_bytes = 4096    # the largest set_parameters request
```

`nros build` refuses the image until they are stated, naming the image, each missing line and a suggested floor. Raising a `params` value raises the `service_inbox_bytes` it needs; the build names the new value.
