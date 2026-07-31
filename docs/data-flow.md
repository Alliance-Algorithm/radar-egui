# Radar 系统数据流文档

综合 **radar-egui**、**alliance_radar_sdr**、**laser_guidance**、**alliance_radar_location_lidar** 描述 RoboMaster 2026 雷达系统完整数据流。

---

## 1. 系统架构总览

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              radar-egui (Rust)                               │
│                              顶层进程控制 + HUD                               │
│                                                                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌─────────────────┐ │
│  │  Laser 标签   │  │   SDR 标签   │  │  Radar 标签   │  │   进程控制       │ │
│  │  视频+目标    │  │ 小地图+面板  │  │  3D 点云     │  │  启停管理        │ │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘  └────────┬────────┘ │
│         │                 │                 │                    │          │
│  ┌──────┴─────────────────┴─────────────────┴────────────────────┴──────┐  │
│  │                       共享状态层 Arc<Mutex<T>>                        │  │
│  │       SharedData（统一） │ LaserObservation │ PointCloudFrame       │  │
│  └──────┬─────────────────┬─────────────────┬────────────────────┬──────┘  │
│         │                 │                 │                    │          │
│  ┌──────┴──────┐  ┌──────┴──────┐  ┌──────┴──────┐  ┌──────────┴──────┐  │
│  │ ZMQ SUB/PUB │  │ Serial RX/TX│  │ Video SHM   │  │ PCD SHM         │  │
│  │ :5555/5556  │  │ (serial2)   │  │ /laser_frame│  │ /pointcloud_    │  │
│  │ :5557/:5558(PUB)  │  │             │  │             │  │ frame           │  │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └──────────┬──────┘  │
└─────────┼─────────────────┼────────────────┼─────────────────────┼─────────┘
          │                 │                │                     │
          ▼                 ▼                ▼                     ▼
┌───────────────────┐ ┌──────────┐ ┌──────────────────┐ ┌──────────────────┐
│ alliance_radar_sdr│ │ DJI      │ │  laser_guidance  │ │  model_to_map    │
│ (Python)          │ │ Referee  │ │  (C++)           │ │  (C++)           │
│ ZMQ PUB :5555     │ │ UART     │ │ SHM /laser_frame │ │  SHM /pointcloud │
│                   │ │          │ │ ZMQ PUB :5556    │ │  _frame          │
└───────────────────┘ └──────────┘ └──────────────────┘ └──────────────────┘
          │                          │
          ▼                          ▼
┌───────────────────┐     ┌───────────────────────┐
│ alliance_radar_   │     │ C++/Python consumers  │
│ location_lidar    │     │ ZMQ SUB :5557/:5558         │
│ (ROS2 Radar)      │     │ 比赛状态/雷达标记等   │
│ ZMQ PUB :5556     │     │                       │
│ (LidarLocation)   │     │                       │
└───────────────────┘     └───────────────────────┘
```

### 1.1 仓库职责

| 仓库 | 语言 | 角色 | 数据输出 |
|------|------|------|----------|
| **radar-egui** | Rust | HUD + 进程编排 + 协议桥接 | ZMQ PUB tcp://*:5557, FIFO `/tmp/laser_cmd`, 串口 TX |
| **alliance_radar_sdr** | Python | SDR 无线信号解析 | ZMQ PUB tcp://127.0.0.1:5555 |
| **laser_guidance** | C++ | 激光目标检测 + 视频推流 | ZMQ PUB :5556 + SHM `/laser_frame` |
| **alliance_radar_location_lidar** | C++/ROS2 | 激光雷达定位 + 相机/融合/桥接（进程控制启动目标） | ZMQ PUB tcp://127.0.0.1:5556（LidarLocation） |
| **model_to_map** | C++ | 场地点云 | SHM `/pointcloud_frame` |

---

## 2. 核心数据流

### 2.1 SDR 无线数据链路（敌方全量数据）

```
alliance_radar_sdr → ZMQ PUB :5555 → radar-egui ZMQ SUB ──直接写──▶ SharedReader 所有的
                                                                      Arc<Mutex<SharedData>>
                                                                                │
                                                                                ▼
                                                                         SDR 标签 (egui)
                                                                         · 小地图(位置)
                                                                         · 血量/弹药/经济/增益面板
```

**数据**：`ReceiveSdr` (JSON, cmd_id=0x2002) → 6 子结构体：

| 子字段 | 内容 | 串口对应 cmd_id |
|--------|------|----------------|
| `position` | 6 机器人 × i16 x/y (cm) | 0x0A01 |
| `blood` | 6 机器人 × u16 (英雄/工程/步兵3/步兵4/预留/哨兵) | 0x0A02 |
| `ammo` | 5 机器人 × u16 (英雄/步兵3/步兵4/无人机/哨兵) | 0x0A03 |
| `state` | 经济(u16×2) + 15 位域状态 (补给站/高地/隧道/增益点) | 0x0A04 |
| `gain` | 5 机器人 × 7B 增益 (hp_recovery/cooling/defence/neg_defence/attack) | 0x0A05 |
| `key` | [u8;6] 干扰密钥 ASCII | 0x0A06 |

### 2.2 串口（裁判系统）数据链路

```
DJI Referee ──UART──▶ Serial RX ──▶ parser ──写──▶ SharedReader 所有的 Arc<Mutex<SharedData>>
                                      │                              │
                                      ├─ idx ──▶ ZMQ PUB             └─▶ Serial UI 独立读取最新快照
                                      │          (查询 SharedData)       (仅串口打开时记录变化)
                                      └─ idx ──▶ Serial TX
                                                 (查询 SharedData → UART)
```

串口 RX/TX 由用户在 Serial UI 点击打开后通过 `open_serial()` 启动，不在 `RadarApp::default` 自动启动。`open_serial()` 把 ZMQ PUB sender 和 Serial TX sender 一并交给 parser，所以每个完成帧的 idx 分别通知这两个消费者；idx 不路由到 UI。

**帧格式**：
```
SOF(1B=0xA5) | data_len(2B LE) | seq(1B) | CRC8(1B) | cmd_id(2B LE) | data[N] | CRC16(2B LE)
```

**RX 协议表**：

| cmd_id | 名称 | 长度 | 字段 |
|--------|------|------|------|
| 0x0001 | 比赛状态 | 11B | game_type(4b) + game_progress(4b) + remain_time(u16) + timestamp(u64) |
| 0x0002 | 比赛结果 | 1B | winner(u8) |
| 0x0101 | 场地事件 | 4B | 14 位域 (补给站/能量机关/高地/增益点/飞镖) |
| 0x0105 | 飞镖发射 | 3B | remain_time(u8) + hit_target(3b) + hit_count(3b) + selected(3b) |
| 0x020C | 雷达标记进度 | 2B | 16×1b 标记/易伤 (详见表↓) |
| 0x020E | 雷达自主决策同步 | 1B | weakness_chance(2b) + active(1b) + encrypt(2b) + modifiable(1b) |
| 0x0301 | 机器人交互 | ≤118B | sub_cmd_id(2B) + sender/receiver(各2B) + sub_data |
| 0x0305 | 小地图雷达 | 48B | 12 机器人 × u16 x/y |

**0x020C 雷达标记进度位域**：
```
bit 0-3: hero/engineer/infantry3/infantry4 敌方易伤 (标记≥100)
bit 4:   opponent_aerial_marked
bit 5:   opponent_sentry_vulnerable
bit 6-9: hero/engineer/infantry3/infantry4 我方标记 (标记≥50)
bit 10:  ally_aerial_marked
bit 11:  ally_sentry_marked
bit 12:  opponent_aerial_targeted   (被我方雷达激光锁定)
bit 13:  opponent_aerial_countered  (敌方无人机反制)
bit 14:  ally_aerial_targeted      (被我方雷达激光锁定)
bit 15:  ally_aerial_countered     (我方无人机反制)
```

**SDR 链路串口字段** (0x0A01-0x0A06)：结构与 §2.1 的 ReceiveSdr 一致，经串口中继。

### 2.3 激光引导数据链路

```
laser_guidance ──→ ZMQ PUB :5556 → radar-egui (ReceiveLaser JSON)
              ──→ SHM /laser_frame → VideoRuntime (BGR8 视频帧)
              ←── FIFO /tmp/laser_cmd (配置/启停)
```

**`ReceiveLaser`** (JSON, cmd_id=0x2003)：`detected` + `center` + `brightness` + `contour` + `candidates[]` (score/class_id/bbox/center)

**视频 SHM**：magic `0x4C465248("LFRH")`，双缓冲 BGR8，atomic frame_seq + write_idx 无锁同步。

**进程控制**：通过 `ScriptRunner` spawn `.script/competition-laser`、`.script/preview-laser`、`.script/stream` 或 `.script/record`，并通过 FIFO 下发 enemy/stream/record 配置。默认 root 为 manifest 相对 `../../laser_guidance`，可用 `LASER_GUIDANCE_ROOT` 覆盖；HikCamera 由 `laser_guidance` 配置和持有，单设备时自动选择。

### 2.4 激光雷达定位数据链路（ROS2 Radar）

**进程控制启动**（`ScriptRunner::start_radar`）：默认 root 为 manifest 相对 `../../alliance_radar_location_lidar`，可用 `ALLIANCE_RADAR_LOCATION_LIDAR_ROOT` 覆盖。启动前校验 workspace setup 和 launch 文件。

```
source /opt/ros/jazzy/setup.bash
source ros_ws/install/setup.bash
exec ros2 launch radar_bringup competition.launch.py side:=<red|blue> enable_raw_recording:=<true|false>
```

`enable_raw_recording` 由 UI「Radar 相机内录」选项控制，默认 `false`；录制目录、分辨率、码率等参数由 radar 仓库 `competition.launch.py` 内部默认值管理。

**数据链路**：

```
alliance_radar_location_lidar (ROS2: camera + lidar + fusion + bridge)
    → ZMQ PUB :5556 → radar-egui ZMQ SUB ──直接写──▶ SharedReader 所有的 Arc<Mutex<SharedData>>
```

**`ReceiveLidarLocation`** (JSON, cmd_id=0x2001)：12 机器人 × (x: u16, y: u16) = 24 字段。
详情见 `docs/lidar-location-protocol.md`。

### 2.5 点云数据链路

```
model_to_map ──→ SHM /pointcloud_frame → PointCloudRuntime ──→ egui 点云状态
                                                            └─→ 可选 Rerun 3D Viewer
```

**点云 SHM**：magic `0x50434446("PCDF")`，双缓冲，每点 28B (x: f32, y: f32, z: f32, rgba: u32, nx/ny/nz: f32)。
坐标映射：`SHM.xyz = (-PCD.x, PCD.z, PCD.y)`。
Rerun/gRPC 仅是可选可视化输出，不是 ROS2 Radar、LidarLocation 或点云 SHM 的业务传输，UI 不据此推断这些链路的连接状态。

---

## 3. ZMQ 通信协议

### 3.1 消息 ID 空间

| 常量 | 值 | 方向 | 消息类型 | 说明 |
|------|----|------|----------|------|
| `GAME_STATE_CMD_ID` | 0x0001 | Rust → | `TransmitGameState` | PUB JSON 复用 DJI 比赛状态 ID |
| `RADAR_MARK_PROCESS_CMD_ID` | 0x020C | Rust → | `TransmitRadarMarkProcess` | PUB JSON 复用 DJI 雷达标记 ID |
| `ZMQ_SUB_LIDAR_LOCATION` | 0x2001 | → Rust | `ReceiveLidarLocation` | 激光定位 |
| `ZMQ_SUB_SDR` | 0x2002 | → Rust | `ReceiveSdr` | SDR 全量 |
| `ZMQ_SUB_LASER` | 0x2003 | → Rust | `ReceiveLaser` | 激光观测 |

- SUB 连接到 `tcp://127.0.0.1:5555` + `:5556`，PUB 绑定 `tcp://*:5557`
- 格式：JSON (serde_json)，SUB 接收超时 100ms
- PUB 没有单独的 `0x1001`/`0x1002` ZMQ ID；不要为 GameState/RadarMark invent 新协议值

### 3.2 串口 ↔ ZMQ 桥接

```
Serial RX → parser → SharedData + tx.send(idx) ─┬→ ZMQ PUB 查询 SharedData → JSON (:5557/:5558)
                                                └→ Serial TX 查询 SharedData → serial_package() → UART TX
ZMQ SUB ← JSON → SharedData → UI 最新快照
```

Parser 的 `usize` 通知索引只决定已完成帧触发哪类发布/发送；`SharedData` 始终是数据真源。Serial TX 阻塞等待自己的 idx receiver；ZMQ SUB 当前只写 `SharedData`，没有连接到该 sender，所以 ZMQ 接收不会触发 UART 发送。协议索引包括：
```
0=GAME_STATE 1=GAME_RESULT 2=SITE_EVENT 3=DART 4=RADAR_MARK
5=RADAR_SYNC 6=ROBOT_INTERACT 7=RADAR_DECISION 8=MINIMAP
9-14=SDR 位置/血量/弹药/状态/增益/密钥
```

---

## 4. 进程控制

```text
egui → tokio::sync::mpsc<ProcessCommand> → Tokio ProcessRuntime actor → ScriptRunner
                                                   │
                                                   └→ watch<ProcessSnapshot> → egui
```

`ProcessControl` 是非阻塞 facade：UI 只发送命令和读取最新 snapshot。actor 在一个专用 OS 线程/一个 Tokio runtime 上独占 `ScriptRunner`。Start All 的 coroutine 状态机通过 `tokio::select!` 同时等待命令、启动间隔和 FIFO 重试 deadline，所以 Stop All/Shutdown 在等待期间可取消后续步骤；不存在由 egui frame polling 驱动的 `PendingStartAll` 路径。

全局 `TeamSide` 表示我方阵营并同步到 `SharedData.radar_side`：Radar 使用我方 side；SDR 使用 `side.enemy()`；Laser 使用 `enemy red|blue`，Auto 模式改用 `enemy auto`。

| UI 动作 | 代码 | 外部进程 |
|---------|------|----------|
| Start SDR | `start_sdr(enemy)` | `../alliance_radar_sdr` → `python3 thread_init.py --enemySide …` |
| Start Radar | `start_radar(side, record)` | 已校验 Radar root → `ros2 launch radar_bringup competition.launch.py side:=… enable_raw_recording:=…` |
| Start Laser | `start(Competition|…)` | 已校验 Laser root 的当前 `.script/…` + FIFO `/tmp/laser_cmd` |
| Start All | `start_all(StartAllOptions)` | Radar → 1s → SDR → 1s → Competition Laser → FIFO 配置 |
| Retry Failed | `retry_failed()` | 从失败的 sequence step 继续，不重启已完成的前序步骤 |
| Stop All | `stop_all()` | 取消 pending 状态，停 Laser → SDR → Radar |

---

## 5. 数据流向总表

| 源 | 目标 | 协议 | 地址 | 数据 |
|----|------|------|------|------|
| `alliance_radar_sdr` | radar-egui | ZMQ | :5555 | ReceiveSdr JSON |
| `alliance_radar_location_lidar` | radar-egui | ZMQ | :5556 | ReceiveLidarLocation JSON |
| `laser_guidance` | radar-egui | ZMQ | :5556 | ReceiveLaser JSON |
| `laser_guidance` | radar-egui | SHM | `/laser_frame` | BGR8 视频 |
| `model_to_map` | radar-egui | SHM | `/pointcloud_frame` | PCD + 法向量 |
| DJI Referee | radar-egui | UART | 串口 | 裁判协议帧 |
| radar-egui | laser_guidance | FIFO | `/tmp/laser_cmd` | 配置命令 |
| radar-egui | 外部 | ZMQ PUB | :5557/:5558 | TransmitGameState/Mark JSON |
| radar-egui | DJI Referee | UART | 串口 | 中继帧 |

---

## 6. 关键设计

| 决策 | 理由 |
|------|------|
| Arc\<Mutex\<T\>\> vs channel | 业务数据由 UI 读取最新快照，Mutex 提供单一可信状态 |
| mpsc command + watch snapshot | 进程命令逐条串行处理，UI 状态读取不消费事件且始终取最新值 |
| `tokio::select!` orchestration | Start All 延迟和 FIFO 重试期间仍可处理取消/关闭命令 |
| 滑动窗口解析器 | 无固定帧分隔符，匹配 DJI 协议，CRC 双重校验 |
| idx notification + SharedData | Parser 只发送已完成帧索引，消费者从唯一可信共享状态读取数据 |
| SHM 双缓冲 | atomic frame_seq + write_idx 无锁同步，零拷贝 |
| ZMQ JSON | 自动重连 + 跨语言 (Rust/Python/C++) |

---

## 7. 模块索引

| 模块 | 文件 | 职责 |
|------|------|------|
| 入口 | `src/main.rs` | eframe 窗口 1280×720 |
| 全局状态 | `src/state.rs` | `SharedReader`/`SharedWriter` 统一持有 `SharedData`；另有 Laser/PointCloud 读写端 |
| 应用 | `src/app/` | RadarApp, 四 workspace 路由, 进程控制 UI, 连接状态, Rerun |
| 运行时 | `src/runtime/mod.rs` | ZmqSub/PubRuntime, VideoRuntime, PointCloudRuntime |
| 进程管理 | `src/services/` | process_runtime(actor/编排) + process_control(UI facade) + script_runner(外部进程/FIFO) |
| 串口 | `src/serial/` + `src/shared_data.rs` | deku 结构体, parser(滑动窗口), package(组帧), crc, serial(I/O 线程) |
| ZMQ | `src/zmq/zmq.rs` | 私有 JSON 消息, PUB/SUB 线程 + SharedData 桥接 |
| 激光 | `src/laser/` | protocol(解析), video(SHM 读取) |
| 点云 | `src/pointcloud/` | protocol(解析), reader(SHM 读取), rerun_visualizer |
| 小地图 | `src/widgets/minimap.rs` | 2D 战场画布, 6 机器人, 拖拽/缩放 |
| 面板 | `src/widgets/panels.rs` | 血量/弹药/经济/增益 状态面板 |
| 激光 workspace | `src/app/laser_*.rs` | 视频舞台、进程控制、HikCamera 所有权说明和分析 |
