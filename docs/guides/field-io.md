# Reading a field device

How a project's field I/O is configured, applied and checked, with the Modbus TCP driver as the example. The workbench's Devices & networks editor writes the same configuration. Back to the [README](../../README.md).

Field I/O is configured per project and runs for the **active** project. The configuration is
stored as `<storage root>/<project id>/io.json`.

Save it with `PUT /api/v1/projects/<project id>/io`. The server checks it first — its own
rules, each driver's view of its connection, and that no tag is provided twice — and refuses
it whole, naming every problem, if anything is wrong. A saved configuration of the active
project is applied at once, and only what differs changes: a connection that is unchanged keeps
running, one whose tags stay the same is swapped to the new settings without a gap, one whose
tags change is restarted, one that is gone is stopped and its tags removed. The answer's
`changes` lists which connection had which. Activating another project switches field I/O to that project the same way.
`GET /api/v1/projects/<project id>/io` reads it back and `GET /api/v1/io/drivers` lists the
drivers it may name. A file written by hand is picked up when the server starts or the project
is activated.

```json
{
  "connections": [
    {
      "id": "boiler-plc",
      "driver": "modbus",
      "poll_interval_ms": 500,
      "config": {
        "transport": "tcp",
        "host": "192.0.2.10",
        "port": 502,
        "slave_id": 1,
        "timeout_ms": 1000,
        "mappings": [
          { "uri": "external.BoilerRoom.PT101.pressure", "address": 100, "function_code": 3, "scale": 0.1 },
          { "uri": "external.BoilerRoom.TT101.temperature", "address": 102, "function_code": 3, "data_type": "f32" }
        ]
      }
    }
  ]
}
```

| Field | Meaning |
| :--- | :--- |
| `poll_interval_ms` | Required. How often the connection is polled. |
| `cycle_timeout_ms` | Optional. Longest a poll cycle may run before every tag of the connection is marked `BAD_COMM_FAILURE` and the driver is restarted. Default: five poll intervals, at least 2000. |
| `function_code` | `3` holding registers, `4` input registers. |
| `data_type` | `u16` (default), `i16`, `u32`, `i32`, `f32`. |
| `word_order` | For two-register types: `high_first` (default) or `low_first`. |
| `scale` | Multiplier applied to the decoded value. Any value other than `1` makes the tag a 64-bit float. |

`GET /api/v1/io/status` reports each connection, and `GET /api/v1/telemetry/stream` carries the
values. A connection that cannot be started is listed with the reason and provides no tags.

Without hardware, a simulated Modbus TCP device can stand in. **Its values are synthetic**; it is
a separate process that is only read when a connection is pointed at it:

```bash
cargo run -p driver-modbus --features sim --example modbus_sim -- 127.0.0.1:1502
```
