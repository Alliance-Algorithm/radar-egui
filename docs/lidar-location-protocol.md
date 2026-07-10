# Lidar Location ZMQ 协议

本文档定义 `alliance_radar_location_lidar` 与 `radar-egui` 之间的激光雷达定位数据传输协议。

## 概述

- **传输方式**: ZMQ PUB/SUB
- **默认地址**: `tcp://127.0.0.1:5556`
- **字节序**: JSON 数字均为小端原生表示
- **发送频率**: 实时（跟随激光雷达扫描帧率）
- **消息 ID**: `0x2001` (`ZMQ_SUB_LIDAR_LOCATION`)

## 数据格式

ZMQ PUB 发送 JSON 字符串，根对象包含 `cmd_id` 和 24 个 `u16` 字段，
分别表示敌方和我方各 6 个机器人的 (x, y) 坐标（单位：cm）。

### JSON 结构

```json
{
  "cmd_id": 0x2001,

  "opponent_hero_x": 1500,
  "opponent_hero_y": 800,
  "opponent_engineer_x": 1200,
  "opponent_engineer_y": 600,
  "opponent_infantry_3_x": 800,
  "opponent_infantry_3_y": 400,
  "opponent_infantry_4_x": 600,
  "opponent_infantry_4_y": 200,
  "opponent_aerial_x": 2000,
  "opponent_aerial_y": 0,
  "opponent_sentry_x": 2500,
  "opponent_sentry_y": 500,

  "ally_hero_x": 100,
  "ally_hero_y": 100,
  "ally_engineer_x": 200,
  "ally_engineer_y": 150,
  "ally_infantry_3_x": 300,
  "ally_infantry_3_y": 250,
  "ally_infantry_4_x": 400,
  "ally_infantry_4_y": 350,
  "ally_aerial_x": 500,
  "ally_aerial_y": 450,
  "ally_sentry_x": 600,
  "ally_sentry_y": 550
}
```

### 字段对照表

| 字段 | 类型 | 说明 |
|------|------|------|
| `cmd_id` | u16 | 固定 `0x2001` |
| `opponent_hero_x/y` | u16 | 敌方英雄坐标 (cm) |
| `opponent_engineer_x/y` | u16 | 敌方工程坐标 |
| `opponent_infantry_3_x/y` | u16 | 敌方步兵3坐标 |
| `opponent_infantry_4_x/y` | u16 | 敌方步兵4坐标 |
| `opponent_aerial_x/y` | u16 | 敌方无人机坐标 |
| `opponent_sentry_x/y` | u16 | 敌方哨兵坐标 |
| `ally_hero_x/y` | u16 | 我方英雄坐标 |
| `ally_engineer_x/y` | u16 | 我方工程坐标 |
| `ally_infantry_3_x/y` | u16 | 我方步兵3坐标 |
| `ally_infantry_4_x/y` | u16 | 我方步兵4坐标 |
| `ally_aerial_x/y` | u16 | 我方无人机坐标 |
| `ally_sentry_x/y` | u16 | 我方哨兵坐标 |

## Rust 接收结构体 (radar-egui)

```rust
// src/zmq/data_format.rs
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReceiveLidarLocation {
    pub cmd_id: u16,
    pub opponent_hero_x: u16,
    pub opponent_hero_y: u16,
    pub opponent_engineer_x: u16,
    pub opponent_engineer_y: u16,
    pub opponent_infantry_3_x: u16,
    pub opponent_infantry_3_y: u16,
    pub opponent_infantry_4_x: u16,
    pub opponent_infantry_4_y: u16,
    pub opponent_aerial_x: u16,
    pub opponent_aerial_y: u16,
    pub opponent_sentry_x: u16,
    pub opponent_sentry_y: u16,
    pub ally_hero_x: u16,
    pub ally_hero_y: u16,
    pub ally_engineer_x: u16,
    pub ally_engineer_y: u16,
    pub ally_infantry_3_x: u16,
    pub ally_infantry_3_y: u16,
    pub ally_infantry_4_x: u16,
    pub ally_infantry_4_y: u16,
    pub ally_aerial_x: u16,
    pub ally_aerial_y: u16,
    pub ally_sentry_x: u16,
    pub ally_sentry_y: u16,
}
```

## Rust 接收示例 (radar-egui)

```rust
// src/zmq/zmq.rs — ZMQ SUB 线程自动反序列化
use serde_json;

if let Ok(lidar) = serde_json::from_slice::<ReceiveLidarLocation>(&bytes) {
    if let Ok(mut z) = zmq_data.lock() {
        z.lidar = Some(lidar);
        z.zmq_produce[IDX_ZMQ_LIDAR] = 1;
    }
    continue;
}
```

## 数据消费

`ReceiveLidarLocation` 与 `ReceiveSdr.position` 结构相似（前者含敌方+我方，后者仅敌方）。
二者可通过 `zmq_sdr_lidar_fusion()` 融合互补：

```rust
// 融合逻辑（src/zmq/zmq.rs）
pub fn zmq_sdr_lidar_fusion(zmq_data: &Arc<Mutex<ZmqData>>) {
    let zmq_lock = zmq_data.lock().unwrap();
    if zmq_lock.zmq_produce[IDX_ZMQ_LIDAR] != 0 && zmq_lock.zmq_produce[IDX_ZMQ_SDR] != 0 {
        // SDR position + Lidar position 融合
    }
}
```

## Python 发送测试

```python
import zmq
import json

context = zmq.Context()
pub = context.socket(zmq.PUB)
pub.bind("tcp://*:5556")

msg = {
    "cmd_id": 0x2001,
    "opponent_hero_x": 1500,
    "opponent_hero_y": 800,
    # ... 填充全部 24 字段 ...
}

pub.send_string(json.dumps(msg))
print("Sent LidarLocation test message")
```

## ZMQ 通信拓扑

```
┌──────────────────────────┐
│ alliance_radar_location_ │
│ lidar (C++)              │
│                           │
│ ZMQ PUB tcp://*:5556     │
└──────────┬───────────────┘
           │ JSON (ReceiveLidarLocation)
           ▼
┌──────────────────────────┐
│ radar-egui               │
│ ZMQ SUB tcp://127.0.0.1 │
│ :5556                    │
│                          │
│ → Arc<Mutex<ZmqData>>   │
│   .lidar                 │
└──────────────────────────┘
```

注意：`laser_guidance` 也在同一端口 `:5556` 上 PUB `ReceiveLaser`，
radar-egui 通过 JSON 中的 `cmd_id` 字段区分两种消息类型。
