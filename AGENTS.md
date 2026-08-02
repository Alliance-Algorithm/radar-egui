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
│  │ :5557/:5558(PUB)  │  │             │  │ (懒启动)    │  │ frame (懒启动)  │  │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
```

## 外部进程与数据源

| 组件 | 仓库 / 路径 | 进程控制如何启动 | 数据输出 |
|------|-------------|------------------|----------|
| SDR 桥 | `../alliance_radar_sdr` | `python3 thread_init.py --enemySide …` | ZMQ PUB `tcp://127.0.0.1:5555`（ReceiveSdr） |
| ROS2 Radar | `ALLIANCE_RADAR_LOCATION_LIDAR_ROOT`，否则 manifest 相对 `../../alliance_radar_location_lidar` | source Jazzy/workspace Bash setup，`ros2 launch radar_bringup competition.launch.py side:=…` | ZMQ PUB `tcp://127.0.0.1:5556`（ReceiveLidarLocation） |
| Laser | `LASER_GUIDANCE_ROOT`，否则 manifest 相对 `../../laser_guidance` | `.script/{competition-laser,preview-laser,stream,record}` + FIFO `/tmp/laser_cmd` | ZMQ PUB `:5556` + SHM `/laser_frame` |
| 点云 | `model_to_map` 等 | 不由 egui 直接 spawn | SHM `/pointcloud_frame` |
| 裁判系统 | UART 设备 | Serial UI 的 `open_serial()` 按钮按需启动 RX/TX 线程（不在 `RadarApp::default` 自动启动） | DJI 协议帧 |

**已废弃 / 勿再写进架构：**

- TCP `127.0.0.1:2000` + `tcp_client.rs` / `protocol.rs` / `RoboMasterSignalInfo`（已由 ZMQ `ReceiveSdr` 替代）
- 禁止重新引入 Unity `RADAR_APP` 作为「Start Radar」目标；当前目标是 `alliance_radar_location_lidar` ROS2 workspace
- Laser UDP observer 作为主路径（代码可能仍在，`ensure_laser_started` 为空；主路径是 ZMQ SUB）

## 数据流

```
alliance_radar_sdr ──ZMQ PUB :5555──┐
laser_guidance ─────ZMQ PUB :5556──┼──▶ ZMQ SUB 线程 ──直接写──▶ SharedReader 所有的
alliance_radar_location_lidar ─────┘                         Arc<Mutex<SharedData>>
                                       │                               │
                                       │  ┌─ tx.send(IDX_ROBOT_INTERACTION) ─▶ Serial TX
                                       │  │   SDR: 0x0121 决策单帧(优先) + 0x0200 广播 5 帧
                                       │  └─ tx.send(IDX_MINIMAP_RECEIVE_RADAR)─▶ Serial TX
                                       │      Lidar: 0x0305 小地图单帧
                                       └─▶ UI 最新快照

DJI Referee ──UART──▶ Serial RX ──▶ Parser ──写──▶ SharedData ──▶ UI 独立读取最新快照
                                      │
                                      └─ tx.send(idx) ──▶ ZMQ PUB 线程 ──▶ :5557/:5558
                                                           TransmitGameState /
                                                           TransmitRadarMarkProcess
（0x0002/0x0101/0x0105/0x020E 只写 SharedData 不通知；0x020E 供自主决策读取）

laser_guidance ──SHM /laser_frame──▶ VideoRuntime (懒) ──▶ Laser 视频
model_to_map ──SHM /pointcloud_frame──▶ PointCloudRuntime (懒) ──▶ egui SHM/点数/帧状态
                                                                  └─▶ 可选 Rerun 3D 可视化
```

### ZMQ 消息 ID

| 常量 | 值 | 方向 | 类型 |
|------|----|------|------|
| `ZMQ_SUB_LIDAR_LOCATION` | 0x2001 | 外部 → Rust | `ReceiveLidarLocation` |
| `ZMQ_SUB_SDR` | 0x2002 | 外部 → Rust | `ReceiveSdr` |
| `ZMQ_SUB_LASER` | 0x2003 | 外部 → Rust | `ReceiveLaser` |

PUB 消息（GameState / RadarMarkProcess）的 JSON 中包含 `cmd_id` 字段，其值复用 `shared_data.rs` 中定义的 DJI 协议 `CMD_ID` 常量，不另设 ZMQ 专用 ID。

默认：SUB 连接 `tcp://127.0.0.1:5555` + `5556`；PUB 绑定 `tcp://*:5557` + `tcp://*:5558`。

## 启动编排

### 进程内（`RadarApp::default`）

1. `SharedReader::new_pair` 创建并持有统一 `Arc<Mutex<SharedData>>`；另建 Laser / Video / PointCloud reader-writer pairs
2. **立即** `ZmqSubRuntime::start` + `ZmqPubRuntime::start`（`std::thread` + 阻塞 `zmq2`，**不是** Tokio 任务）
3. `VideoRuntime` / `PointCloudRuntime` 只构造，进对应标签时 `ensure_started`
4. `ProcessControl::new` 启动专用 `ProcessRuntime` actor；actor 空闲等待 UI 的 `ProcessCommand`

### 外部进程（UI）

```text
egui → tokio::sync::mpsc<ProcessCommand> → ProcessRuntime actor → ScriptRunner
                                                   │
                                                   └→ watch<ProcessSnapshot> → egui
```

全局 `TeamSide` 表示我方阵营并同步到 `SharedData.radar_side`。Radar 接收我方 `red|blue`；SDR 接收相反阵营作为 `--enemySide`；Laser 默认接收相反阵营的 `enemy red|blue`，开启 Auto 时接收 `enemy auto`。HikCamera 由 `laser_guidance` 配置和持有，单设备时由其自动选择，egui 不下发 camera device。

| 动作 | 行为 |
|------|------|
| Start SDR | `ScriptRunner::start_sdr` |
| Start Radar | `ScriptRunner::start_radar` → 已校验的 ROS2 Radar root |
| Start Laser | laser 脚本 + 可选 FIFO 配置 |
| Start All | actor 协程：Radar → 1s → SDR → 1s → Laser::Competition → FIFO |
| Stop All | 取消未完成的 Start All/FIFO 重试，再停 Laser → SDR → Radar |

## 运行时模型（Tokio）

- **依赖有** `tokio` full；**主 I/O 路径不全是 Tokio**
- Process runtime：一个专用 OS 线程 + 一个 Tokio runtime；actor 独占同步 `ScriptRunner`
- 进程命令：unbounded Tokio `mpsc`；进程状态：`watch<ProcessSnapshot>`
- Start All/FIFO deadline 与命令接收由 `tokio::select!` 竞争，延迟期间可响应 Stop All/Shutdown
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
- `SharedReader` 持有统一 `Arc<Mutex<SharedData>>`，向 UI 提供最新快照；ZMQ/Serial runtime 共享其 `inner()`

### `runtime/mod.rs`
- `ZmqSubRuntime` / `ZmqPubRuntime`
- `VideoRuntime` / `PointCloudRuntime` + `spawn_runtime_task`

### `services/`
- `script_runner.rs`：spawn/kill SDR、ROS2 Radar、laser 脚本
- `process_runtime.rs`：Tokio actor、`ProcessCommand`、`ProcessSnapshot`、Start All/cancellation/FIFO retry 编排
- `process_control.rs`：egui 使用的非阻塞 facade，只入队命令并读取 snapshot

### `zmq/`
- `zmq.rs`：私有 JSON 接收类型、SUB/PUB 线程；SUB 解码后直接写统一 `SharedData`

### `serial/`
- DJI 裁判协议：parser / package / CRC / deku 结构体
- Serial UI 调用 `open_serial()` 后启动 `serial_start_receiver` / `serial_start_transmitter`；`RadarApp::default` 不自动打开串口
- Parser 通过 `mpsc::Sender` 通道只通知 ZMQ PUB 线程（0x0001 GameState / 0x020C RadarMarkProcess）；其余 RX 帧（0x0002/0x0101/0x0105/0x020E）只写 SharedData，不回发 UART
- Serial TX 阻塞等待自己的 idx receiver，由 ZMQ SUB 通知驱动（SDR → 0x0121 决策单帧优先 + 0x0200 广播 5 帧；Lidar → 0x0305 单帧）；RX 帧不回发
- Serial 无任何 I/O timeout（无延时）；ZMQ SUB 阻塞 `recv_bytes`，`ZmqSubRuntime::stop()` 不 join（detach）

### `laser/` / `pointcloud/` / `widgets/`
- 视频 SHM、点云 SHM、小地图、状态面板、Laser 面板
- 可选 `rerun` feature 只做 3D 可视化，不代表 ROS2 Radar、ZMQ 或 SHM 连接状态

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

1. 串口是已接线的用户启动路径：Serial UI 的 `open_serial()` 启动 RX/TX；设计上不在 `RadarApp::default` 自动启动
2. ZMQ SUB 阻塞 `recv_bytes` 无接收超时，`ZmqSubRuntime::stop()` 不 join（detach，线程随进程退出）；串口 RX 线程无 timeout，close 时可能阻塞在 read（设备有数据时下一帧后退出）
3. `AGENTS.md` 旧版 TCP:2000 描述已废弃——以本文件与 `docs/data-flow.md` 为准
