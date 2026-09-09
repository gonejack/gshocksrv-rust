# gshocksrv

![Rust version](https://img.shields.io/badge/rust-1.85%2B-orange.svg)
[![Build](https://github.com/gonejack/gshocksrv-rust/actions/workflows/release.yml/badge.svg)](https://github.com/gonejack/gshocksrv-rust/actions/workflows/release.yml)
[![GitHub license](https://img.shields.io/github/license/gonejack/gshocksrv-rust.svg?color=blue)](LICENSE)

[English](#english) | [中文](#中文)

## English

`gshocksrv` is a Rust command-line Bluetooth time server for G-Shock watches. It waits for a compatible Casio watch to connect over Bluetooth, then synchronizes the watch with the host's local time.

> **Project origin: this project is adapted and ported from the headless version of [izivkov/GShockTimeServer](https://github.com/izivkov/GShockTimeServer). It focuses exclusively on unattended command-line use and does not include the original project's LCD/display interface.**

### Features

- Discovers and connects to watches advertising the Casio time service
- Handles standard digital, analogue, and MIP time-setting protocols
- Supports sequential connections and automatic time sync for multiple watches
- Applies a fine adjustment from `-10` to `+10` seconds
- Rate-limits watches that advertise continuously so they do not block other watches
- Atomically records the last connection time and watch name in a JSON state file
- Uses native Bluetooth backends on Linux, macOS, and Windows
- Runs headlessly with no display or web interface

### Compatibility

The implementation follows GShockTimeServer's protocol behavior and includes dedicated profiles for these watch families:

| Type | Configured model examples |
| --- | --- |
| Standard digital | GW-B5600, GMW-B5000, MRG-B5000, GCW-B5000, DW-B5600, and others |
| Analogue | MTG-B1000, MTG-B3000, MTG-B3100 |
| MIP | GW-BX5600 |
| Continuously advertising | DW-H5600, GBD-H2000, DW-GH5600, GM-H5600, and ECB models |

Other watches using the same Casio BLE time service and standard protocol may also work. Compatibility here means time synchronization, not support for every fitness, reminder, or alarm feature. Some models have not yet been tested with physical hardware against this Rust port.

### Installation

Choose either of the following installation methods. You do not need to complete both.

#### Option 1: Install a prebuilt binary

Download and extract the archive for your platform from [Releases](https://github.com/gonejack/gshocksrv-rust/releases):

| Platform | Release asset |
| --- | --- |
| Linux x86_64 | `gshocksrv-vX.Y.Z-linux-x86_64.zip` |
| Linux ARM64 | `gshocksrv-vX.Y.Z-linux-aarch64.zip` |
| macOS Intel | `gshocksrv-vX.Y.Z-macos-x86_64.zip` |
| macOS Apple Silicon | `gshocksrv-vX.Y.Z-macos-aarch64.zip` |
| Windows x86_64 | `gshocksrv-vX.Y.Z-windows-x86_64.zip` |

#### Option 2: Build from source

Rust 1.85 or newer is required:

```bash
git clone https://github.com/gonejack/gshocksrv-rust.git
cd gshocksrv-rust
cargo build --locked --release
```

The binary is written to `target/release/gshocksrv`, or `target\release\gshocksrv.exe` on Windows.

### Linux requirements

Linux requires BlueZ and D-Bus. On Debian, Ubuntu, or Raspberry Pi OS, install them with:

```bash
sudo apt-get update
sudo apt-get install -y bluez dbus
```

### Usage

Start the server:

```bash
./gshocksrv
```

Short-press the watch's lower-right button or long-press its lower-left button to connect. Once connected, the server sets the time and disconnects. Watches with automatic time adjustment enabled can also connect on their own.

Common options:

| Option | Default | Description |
| --- | --- | --- |
| `--fine-adjustment-secs <SECONDS>` | `0` | Add an offset of `-10..10` seconds to the synchronized time |
| `--scan-timeout <DURATION>` | `60s` | Maximum duration of each Bluetooth scan |
| `--request-timeout <DURATION>` | `5s` | Maximum time to wait for a watch response |
| `--store-path <PATH>` | `gshock_server_data.json` | State-file path |
| `--log-level <LEVEL>` | `info` | `trace`, `debug`, `info`, `warn`, or `error` |
| `--no-color` | off | Disable colored log output |

For example, add two seconds and enable debug logging:

```bash
./gshocksrv --fine-adjustment-secs 2 --log-level debug
```

Press `Ctrl+C` to stop cleanly. macOS and Windows may ask for Bluetooth permission on first launch. On Linux, ensure that Bluetooth is running and that the current user can access BlueZ.

### Testing

```bash
cargo fmt --check
cargo test --locked
```

### License

MIT

---

## 中文

`gshocksrv` 是一个使用 Rust 编写的 G-Shock 蓝牙校时命令行服务。启动后，它会等待兼容的 Casio 手表通过蓝牙连接，并自动将本机时间同步到手表。

> **项目来源：本项目借鉴并移植自 [izivkov/GShockTimeServer](https://github.com/izivkov/GShockTimeServer) 的无界面（headless）版本，专注于纯命令行、长期后台运行的使用场景。本项目不包含原项目的 LCD/显示屏界面。**

### 功能

- 自动发现并连接广播 Casio 时间服务的手表
- 支持标准数字表、指针表和 MIP 表的校时协议
- 支持多只手表依次连接和自动校时
- 支持 `-10` 到 `+10` 秒的精细时间偏移
- 对持续广播的表款限制连接频率，避免影响其他手表
- 将最后连接时间和表名原子写入 JSON 状态文件
- 支持 Linux、macOS 和 Windows 的原生蓝牙后端
- 无界面、无 Web 服务，适合桌面电脑或树莓派长期运行

### 兼容性

本项目沿用了 GShockTimeServer 的协议行为，并为以下类型提供了专用配置：

| 类型 | 已配置的表款示例 |
| --- | --- |
| 标准数字表 | GW-B5600、GMW-B5000、MRG-B5000、GCW-B5000、DW-B5600 等 |
| 指针表 | MTG-B1000、MTG-B3000、MTG-B3100 |
| MIP | GW-BX5600 |
| 持续广播连接 | DW-H5600、GBD-H2000、DW-GH5600、GM-H5600、ECB 系列 |

其他使用相同 Casio BLE 时间服务和标准协议的表款也可能正常工作。兼容表示可以完成校时，不表示运动、提醒、闹钟等所有手表功能均受支持；部分表款尚未在本 Rust 版本中实机验证。

### 安装

以下两种安装方式任选其一，无需依次执行。

#### 方式一：安装预编译二进制

从 [Releases](https://github.com/gonejack/gshocksrv-rust/releases) 下载与你的平台对应的压缩包并解压：

| 平台 | Release 产物 |
| --- | --- |
| Linux x86_64 | `gshocksrv-vX.Y.Z-linux-x86_64.zip` |
| Linux ARM64 | `gshocksrv-vX.Y.Z-linux-aarch64.zip` |
| macOS Intel | `gshocksrv-vX.Y.Z-macos-x86_64.zip` |
| macOS Apple Silicon | `gshocksrv-vX.Y.Z-macos-aarch64.zip` |
| Windows x86_64 | `gshocksrv-vX.Y.Z-windows-x86_64.zip` |

#### 方式二：从源码构建

需要 Rust 1.85 或更高版本：

```bash
git clone https://github.com/gonejack/gshocksrv-rust.git
cd gshocksrv-rust
cargo build --locked --release
```

生成的程序位于 `target/release/gshocksrv`（Windows 为 `target\release\gshocksrv.exe`）。

### Linux 运行依赖

Linux 需要 BlueZ 和 D-Bus。在 Debian、Ubuntu 或 Raspberry Pi OS 上执行：

```bash
sudo apt-get update
sudo apt-get install -y bluez dbus
```

### 使用

启动服务：

```bash
./gshocksrv
```

然后短按手表右下按钮或长按左下按钮发起连接。连接成功后，程序会设置手表时间并断开连接；启用了自动校时的手表也可自行连接。

常用参数：

| 参数 | 默认值 | 说明 |
| --- | --- | --- |
| `--fine-adjustment-secs <SECONDS>` | `0` | 在同步时间上增加 `-10..10` 秒偏移 |
| `--scan-timeout <DURATION>` | `60s` | 每次蓝牙扫描的最长时间 |
| `--request-timeout <DURATION>` | `5s` | 等待手表响应的最长时间 |
| `--store-path <PATH>` | `gshock_server_data.json` | 状态文件路径 |
| `--log-level <LEVEL>` | `info` | `trace`、`debug`、`info`、`warn` 或 `error` |
| `--no-color` | 关闭 | 禁用彩色日志 |

例如，将校时时间提前 2 秒并启用调试日志：

```bash
./gshocksrv --fine-adjustment-secs 2 --log-level debug
```

使用 `Ctrl+C` 可安全停止服务。macOS 和 Windows 首次启动时可能需要授予程序蓝牙权限；Linux 上请确认蓝牙服务已启动，并且当前用户有权访问 BlueZ。

### 测试

```bash
cargo fmt --check
cargo test --locked
```

### 许可证

MIT
