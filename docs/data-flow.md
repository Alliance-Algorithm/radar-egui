# Radar 系统数据流文档

综合 **radar-egui**、**alliance_radar_sdr**、**laser_guidance**（及 **RADAR_APP**、**alliance_radar_location_lidar**）描述 RoboMaster 2026 雷达系统完整数据流。

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
│  │     ZmqData  │  SerialData  │  LaserObservation  │  PointCloudFrame  │  │
│  └──────┬─────────────────┬─────────────────┬────────────────────┬──────┘  │
│         │                 │                 │                    │          │
│  ┌──────┴──────┐  ┌──────┴──────┐  ┌──────┴──────┐  ┌──────────┴──────┐  │
│  │ ZMQ SUB/PUB │  │ Serial RX/TX│  │ Video SHM   │  │ PCD SHM         │  │
│  │ :5555/5556  │  │ (serial2)   │  │ /laser_frame│  │ /pointcloud_    │  │
│  │ :5557(PUB)  │  │             │  │             │  │ frame           │  │
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
┌───────────────────┐     ┌───────────────────┐     ┌───────────────────────┐
│ alliance_radar_   │     │ RADAR_APP (Unity) │     │ C++/Python consumers │
│ location_lidar    │     │ ZMQ SUB :5557     │     │ ZMQ SUB :5557         │
│ ZMQ PUB :5556     │     │ 3D 可视化         │     │ 外部数据消费          │
└───────────────────┘     └───────────────────┘     └───────────────────────┘
```

### 1.1 仓库职责

| 仓库 | 语言 | 角色 | 数据输出 |
|------|------|------|----------|
| **radar-egui** | Rust | HUD + 进程编排 + 协议桥接 | ZMQ PUB tcp://*:5557, FIFO `/tmp/laser_cmd`, 串口 TX |
| **alliance_radar_sdr** | Python | SDR 无线信号解析 | ZMQ PUB tcp://127.0.0.1:5555 |
| **laser_guidance** | C++ | 激光目标检测 + 视频推流 | ZMQ PUB :5556 + SHM `/laser_frame` |
| **alliance_radar_location_lidar** | C++ | 激光雷达定位 | ZMQ PUB tcp://127.0.0.1:5556 |
| **model_to_map** | C++ | 场地点云 | SHM `/pointcloud_frame` |
| **RADAR_APP** | C# (Unity) | 3D 战场 | ZMQ SUB :5557 |

---

## 2. 核心数据流

### 2.1 SDR 无线数据链路（敌方全量数据）

```
alliance_radar_sdr → ZMQ PUB :5555 → radar-egui ZMQ SUB → Arc<Mutex<ZmqData>>
                                                                  │
                                  ┌────────────────────────────────┤
                                  ▼                                ▼
                           SDR 标签 (egui)                  ZMQ PUB :5557
                           · 小地图(位置)                   (中继串口数据)
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
DJI Referee ◀════ UART 115200bps ═══▶ radar-egui Serial RX/TX
                                           │
                                    Arc<Mutex<SerialData>>
                                           │
                          ┌────────────────┼────────────────┐
                          ▼                                  ▼
                   ZMQ PUB 线程                           Serial TX 线程
                   (串口→ZMQ:10ms轮询)                    (ZMQ→串口:10ms轮询)
```

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

**进程控制**：通过 `ScriptRunner` spawn `laser_guidance/.script/{competition|preview|stream|record}`，通过 FIFO 下发 enemy/stream/record 配置。

### 2.4 激光雷达定位数据链路

```
alliance_radar_location_lidar → ZMQ PUB :5556 → radar-egui ZMQ SUB
```

**`ReceiveLidarLocation`** (JSON, cmd_id=0x2001)：12 机器人 × (x: u16, y: u16) = 24 字段。
详情见 `docs/lidar-location-protocol.md`。

### 2.5 点云数据链路

```
model_to_map ──→ SHM /pointcloud_frame → PointCloudRuntime → Rerun 3D Viewer
```

**点云 SHM**：magic `0x50434446("PCDF")`，双缓冲，每点 28B (x: f32, y: f32, z: f32, rgba: u32, nx/ny/nz: f32)。
坐标映射：`SHM.xyz = (-PCD.x, PCD.z, PCD.y)`。

---

## 3. ZMQ 通信协议

### 3.1 消息 ID 空间

| 常量 | 值 | 方向 | 消息类型 | 说明 |
|------|----|------|----------|------|
| `ZMQ_PUB_GAME_STATE` | 0x1001 | Rust → | `TransmitGameState` | 比赛阶段 |
| `ZMQ_PUB_RADAR_MARK` | 0x1002 | Rust → | `TransmitRadarMarkProcess` | 雷达标记 |
| `ZMQ_SUB_LIDAR_LOCATION` | 0x2001 | → Rust | `ReceiveLidarLocation` | 激光定位 |
| `ZMQ_SUB_SDR` | 0x2002 | → Rust | `ReceiveSdr` | SDR 全量 |
| `ZMQ_SUB_LASER` | 0x2003 | → Rust | `ReceiveLaser` | 激光观测 |

- SUB 连接到 `tcp://127.0.0.1:5555` + `:5556`，PUB 绑定 `tcp://*:5557`
- 格式：JSON (serde_json)，SUB 接收超时 100ms

### 3.2 串口 ↔ ZMQ 桥接

```
Serial RX → parser → serial_produced[15] → zmq_serial_update() (10ms) → ZMQ PUB (0x1001, 0x1002)
ZMQ SUB ← JSON → zmq_produced[6] → Serial transmitter (10ms) → serial_package() → UART TX
```

**标志位索引**：
```
SerialData:  0=GAME_STATE 1=GAME_RESULT 2=SITE_EVENT 3=DART 4=RADAR_MARK
             5=RADAR_SYNC 6=ROBOT_INTERACT 7=RADAR_DECISION 8=MINIMAP
             9-14=SDR 位置/血量/弹药/状态/增益/密钥
ZmqData:     0=SDR 1=LASER 2=LIDAR 3=GAME_STATE 4=RADAR_MARK
```

---

## 4. "Start All" 流程

```
UI Start All → start_sdr() → 延迟 1s → start Laser::Competition → FIFO 下发配置
时间线: t=0s SDR → t=1s Laser + enemy/stream/record 配置
```

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
| radar-egui | 外部 | ZMQ PUB | :5557 | TransmitGameState/Mark JSON |
| radar-egui | DJI Referee | UART | 串口 | 中继帧 |

---

## 6. 关键设计

| 决策 | 理由 |
|------|------|
| Arc\<Mutex\<T\>\> vs channel | egui 每帧 16ms 读快照，Mutex 保证最新值，无竞争 |
| 滑动窗口解析器 | 无固定帧分隔符，匹配 DJI 协议，CRC 双重校验 |
| 双标志数组 | serial_produced[15] / zmq_produced[6] 分离，避免串口↔ZMQ 阻塞 |
| SHM 双缓冲 | atomic frame_seq + write_idx 无锁同步，零拷贝 |
| ZMQ JSON | 自动重连 + 跨语言 (Rust/Python/C++/C#) |

---

## 7. 模块索引

| 模块 | 文件 | 职责 |
|------|------|------|
| 入口 | `src/main.rs` | eframe 窗口 1280×720 |
| 全局状态 | `src/state.rs` | Zmq/Serial/Laser/PointCloud 读写端 |
| 应用 | `src/app/mod.rs` | RadarApp, 三标签路由, 连接状态, Rerun |
| 侧边栏 | `src/app/view.rs` | 模式栏, SDR/Laser 侧边栏 |
| 运行时 | `src/runtime/mod.rs` | ZmqSub/PubRuntime, VideoRuntime, PointCloudRuntime |
| 进程管理 | `src/services/` | script_runner(SDR/Laser/Unity 启停) + process_control(Start All 编排) |
| 串口 | `src/serial/` | data_format(15 deku 结构体), parser(滑动窗口), package(组帧), crc, serial(I/O 线程) |
| ZMQ | `src/zmq/` | data_format(消息结构体), zmq(PUB/SUB 线程 + 桥接) |
| 激光 | `src/laser/` | protocol(解析), video(SHM 读取) |
| 点云 | `src/pointcloud/` | protocol(解析), reader(SHM 读取), rerun_visualizer |
| 小地图 | `src/widgets/minimap.rs` | 2D 战场画布, 6 机器人, 拖拽/缩放 |
| 面板 | `src/widgets/panels.rs` | 血量/弹药/经济/增益 状态面板 |
| 激光面板 | `src/widgets/laser_panel.rs` | 视频舞台 + 分析 |
