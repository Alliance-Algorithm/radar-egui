# AGENTS.md

本文档描述 radar-egui 的架构和数据流（以当前代码为准）。

更细的跨仓库数据流见 `docs/data-flow.md`；LidarLocation 协议见 `docs/lidar-location-protocol.md`。

## 角色

radar-egui 是比赛系统的 **HUD + 顶层进程控制**：

- 订阅外部 ZMQ 数据（SDR / Laser / LidarLocation）
- 发布裁判相关状态（GameState / RadarMark）到 ZMQ PUB
- 可选串口桥接裁判系统（协议层已实现，启动接线见 `items.md`）
- 一键启停外部进程：SDR、laser_guidance、**ROS2 Radar（`alliance_radar_location_lidar`）**

## 架构总览

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              radar-egui (Rust)                               │
│                              顶层进程控制 + HUD                               │
│                                                                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌─────────────────┐ │
│  │  Laser 标签   │  │   SDR 标签   │  │  Radar 标签   │  │   进程控制       │ │
│  │  视频+目标    │  │ 小地图+面板  │  │  3D 点云     │  │  SDR/Laser/ROS2 │ │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘  └────────┬────────┘ │
│         │                 │                 │                    │          │
│  ┌──────┴─────────────────┴─────────────────┴────────────────────┴──────┐  │
│  │                       共享状态层 Arc<Mutex<T>>                        │  │
│  │     SharedData (统一)  │  LaserObservation  │  PointCloudFrame  │  │
│  └──────┬─────────────────┬─────────────────┬────────────────────┬──────┘  │
│         │                 │                 │                    │          │
│  ┌──────┴──────┐  ┌──────┴──────┐  ┌──────┴──────┐  ┌──────────┴──────┐  │
│  │ ZMQ SUB/PUB │  │ Serial RX/TX│  │ Video SHM   │  │ PCD SHM         │  │
│  │ :5555/5556  │  │ (serial2)   │  │ /laser_frame│  │ /pointcloud_    │  │
│  │ :5557(PUB)  │  │             │  │ (懒启动)    │  │ frame (懒启动)  │  │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
```

## 外部进程与数据源

| 组件 | 仓库 / 路径 | 进程控制如何启动 | 数据输出 |
|------|-------------|------------------|----------|
| SDR 桥 | `../alliance_radar_sdr` | `python3 thread_init.py --enemySide …` | ZMQ PUB `tcp://127.0.0.1:5555`（ReceiveSdr） |
| ROS2 Radar | `../alliance_radar_location_lidar` | `ros2 launch radar_bringup competition.launch.py side:=…`（Jazzy + `ros_ws`） | ZMQ PUB `tcp://127.0.0.1:5556`（ReceiveLidarLocation） |
| Laser | `laser_guidance`（`LASER_GUIDANCE_ROOT` 或相对路径探测） | `.script/{competition,preview,stream,record}` + FIFO `/tmp/laser_cmd` | ZMQ PUB `:5556` + SHM `/laser_frame` |
| 点云 | `model_to_map` 等 | 不由 egui 直接 spawn | SHM `/pointcloud_frame` |
| 裁判系统 | UART 设备 | **串口线程尚未在 app 中启动** | DJI 协议帧 |

**已废弃 / 勿再写进架构：**

- TCP `127.0.0.1:2000` + `tcp_client.rs` / `protocol.rs` / `RoboMasterSignalInfo`（已由 ZMQ `ReceiveSdr` 替代）
- Unity `RADAR_APP` 作为「Start Radar」目标（已由 `alliance_radar_location_lidar` ROS2 替代）
- Laser UDP observer 作为主路径（代码可能仍在，`ensure_laser_started` 为空；主路径是 ZMQ SUB）

## 数据流

```
alliance_radar_sdr ──ZMQ PUB :5555──┐
laser_guidance ─────ZMQ PUB :5556──┼──▶ ZMQ SUB 线程 ──▶ Arc<Mutex<SharedData>>
alliance_radar_location_lidar ─────┘         │
                                             ├─▶ UI (SDR 小地图/面板, Laser 分析)
                                             └─▶ Serial TX (10ms 轮询 SharedData → UART)

DJI Referee ──UART──▶ Serial RX ──▶ Parser ──▶ tx.send(idx) ──▶ ZMQ PUB 线程
                                                                     │
                                                                     └─▶ :5557
                                                                 TransmitGameState /
                                                                 TransmitRadarMarkProcess

laser_guidance ──SHM /laser_frame──▶ VideoRuntime (懒) ──▶ Laser 视频
model_to_map ───SHM /pointcloud_frame──▶ PointCloudRuntime (懒) ──▶ Rerun
```

### ZMQ 消息 ID

| 常量 | 值 | 方向 | 类型 |
|------|----|------|------|
| `ZMQ_SUB_LIDAR_LOCATION` | 0x2001 | 外部 → Rust | `ReceiveLidarLocation` |
| `ZMQ_SUB_SDR` | 0x2002 | 外部 → Rust | `ReceiveSdr` |
| `ZMQ_SUB_LASER` | 0x2003 | 外部 → Rust | `ReceiveLaser` |

PUB 消息（GameState / RadarMarkProcess）的 JSON 中包含 `cmd_id` 字段，其值复用 `shared_data.rs` 中定义的 DJI 协议 `CMD_ID` 常量，不另设 ZMQ 专用 ID。

默认：SUB 连接 `tcp://127.0.0.1:5555` + `5556`；PUB 绑定 `tcp://*:5557`。

## 启动编排

### 进程内（`RadarApp::default`）

1. 创建共享状态：`ZmqData` / `SerialData` / Laser / Video / PointCloud reader-writer pairs
2. **立即** `ZmqSubRuntime::start` + `ZmqPubRuntime::start`（`std::thread` + 阻塞 `zmq2`，**不是** Tokio 任务）
3. `VideoRuntime` / `PointCloudRuntime` 只构造，进对应标签时 `ensure_started`
4. `ProcessControl` 空闲，等 UI 按钮

### 外部进程（UI）

| 动作 | 行为 |
|------|------|
| Start SDR | `ScriptRunner::start_sdr` |
| Start Radar | `ScriptRunner::start_radar` → `../alliance_radar_location_lidar` ROS2 launch |
| Start Laser | laser 脚本 + 可选 FIFO 配置 |
| Start All | t=0 SDR → t=1s Laser::Competition + FIFO（**不含** ROS2 Radar） |
| Stop All | 停 Radar + Laser + SDR |

## 运行时模型（Tokio）

- **依赖有** `tokio` full；**主 I/O 路径不全是 Tokio**
- ZMQ / Serial：`std::thread` 阻塞循环
- Video / PointCloud：`spawn_runtime_task` → 每任务一个 OS 线程 + **独立** `tokio::runtime::Runtime::new().block_on(...)`
- 关闭：Video/PointCloud 用 `watch`；ZMQ 的 `AtomicBool stop` 与工作循环未完全打通（已知缺口）

## 模块职责

### `main.rs`
- eframe 入口，窗口 1280×720，`env_logger`

### `app/`
- `RadarApp`：四标签（Laser / SDR / Radar / Serial）、主题、连接状态、进程控制 UI
- `connection.rs`：按 ZMQ SDR 快照更新连接状态 / Rerun 日志

### `state.rs`
- `SharedReader`/`SharedWriter`、`LaserObservationReader`/`Writer`、`PointCloudFrameReader`/`Writer`
- `SharedReader` 统一 `Arc<Mutex<SharedData>>` 最新值快照

### `runtime/mod.rs`
- `ZmqSubRuntime` / `ZmqPubRuntime`
- `VideoRuntime` / `PointCloudRuntime` + `spawn_runtime_task`

### `services/`
- `script_runner.rs`：spawn/kill SDR、ROS2 Radar、laser 脚本
- `process_control.rs`：Start All 延迟编排、FIFO 命令

### `zmq/`
- `zmq.rs`：init/send/recv、SUB/PUB 线程、`ZmqData` JSON 消息

### `serial/`
- DJI 裁判协议：parser / package / CRC / deku 结构体
- `serial_start_receiver` / `serial_start_transmitter` — app 通过 `open_serial()` 调用
- Parser 通过 `mpsc::Sender` 通道通知 ZMQ PUB 线程

### `laser/` / `pointcloud/` / `widgets/`
- 视频 SHM、点云 SHM、小地图、状态面板、Laser 面板
- 可选 `rerun` feature 做 3D 可视化

## 关键设计决策

### 为什么用 egui 而不是 Rerun 做实时 HUD
- Rerun 日志优先，不适合操作面板
- egui 即时模式，约 10 fps 足够
- Rerun 作可选 3D/录制

### 为什么用 Arc\<Mutex\<T\>\> 而不是 channel
- UI 每帧要最新快照；channel 易丢最新值
- 写少读多，Mutex 竞争可接受

### 为什么用 channel 连接 Parser 和 ZMQ PUB
- Parser 完成一帧解析后通过 `mpsc::Sender<usize>` 发 idx 通知
- ZMQ PUB 线程阻塞 `rx.recv()`，有通知时才查询 SharedData 并发布
- 对比方案：
  - ❌ 脏标志轮询：UI 帧率不可控，可能漏更新
  - ❌ 直接 channel 传数据：SharedData 是唯一可信源
  - ✅ idx channel：精简通知，SharedData 始终是单一真实来源

### 为什么 ROS2 Radar 而不是 Unity
- 定位与融合在 `alliance_radar_location_lidar`（camera + lidar + fusion + bridge）
- egui 只负责 launch 与订阅其 ZMQ 输出

## 构建与运行

```bash
cargo build --release
cargo run --release
RUST_LOG=info cargo run --release
cargo run --release --features rerun   # 可选 3D
```

## 测试

```bash
cargo test
cargo clippy -- -D warnings
```

## 已知缺口（给 agent 的注意点）

1. 串口通过 UI `open_serial()` 按钮调用，未在 `RadarApp::default` 自动启动
2. ~~ZMQ SUB 尚未完整写回 SerialData 做中继~~（已由统一 SharedData 解决：SUB 和 Serial 都写 SharedData，TX 轮询读）
3. ZMQ runtime `stop` 与线程循环未完全联动
4. `AGENTS.md` 旧版 TCP:2000 描述已废弃——以本文件与 `docs/data-flow.md` 为准
