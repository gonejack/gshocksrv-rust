# gshocksrv-rust

Rust implementation of the G-Shock time-server command line and watch protocol.
It mirrors the original Python/Go behavior for CLI validation, fine adjustment,
model profiles, standard/analogue/MIP packets, connection limiting, and atomic
`gshock_server_data.json` state updates.

## Build and test

```text
cargo test
cargo run -- --help
```

The default binary uses `btleplug` for BLE scanning and GATT connections on
Linux (BlueZ), macOS (CoreBluetooth), and Windows. The Bluetooth layer remains
behind the `BluetoothBackend` and `ConnectedWatch` traits so protocol logic can
also be tested without hardware.
