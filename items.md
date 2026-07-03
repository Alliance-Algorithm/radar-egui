# radar-egui TODO

## 串口通信层

- [x] 串口协议解析器 (serial_parser) — 滑动窗口 cmd_id 扫描
- [x] 设备 ID 枚举 (DeviceId) — From<u16> / From<DeviceId> 双向转换
- [x] 机器人交互包收发 (0x0301) — RobotInteractionData + subcontext 解析
- [x] 常规链路协议数据解析 (0x0001–0x020E) + SDR 解析占位
- [x] SerialData + serial_produced[15] / zmq_produced[15] 双向标志数组
- [x] transmitter 重写 — 读取 zmq_produced[idx] → serial_package → 串口发送 → 归 0
- [x] serial_parser 解析后置位 serial_produced[idx] = 1
- [ ] 串口收发线程正式连线 app/mod.rs
- [ ] 串口发送分批次 — 不同 cmd_id 按各自频率独立发送

## ZMQ 通信层

- [x] zmq.rs — zmq_init_pub/zmq_init_sub 拆分、zmq_send、zmq_recv 封装
- [x] serde + serde_json 依赖引入 (Cargo.toml)
- [x] zmq/data_format.rs — ZmqMessageId + Transmit*/Receive* + ZmqData (Option<T>)
- [x] `ZMQ_PUB_*` / `ZMQ_SUB_*` 消息 ID 空间独立定义
- [x] start_zmq_pub — PUB 线程 (zmq_serial_update → serialize → zmq_send) + ZmqPubRuntime 包装
- [x] start_zmq_sub — SUB 线程 (recv_bytes → try SDR/Laser/Lidar → write ZmqData) + ZmqSubRuntime 包装
- [x] zmq_package.rs → 精简为 serialize_to_json 泛型函数
- [x] zmq_parser.rs → 已删除，逻辑内联到 start_zmq_sub
- [ ] zmq_sdr_lidar_fusion — SDR + Lidar 位置融合

## SDR 无线链路（TCP → ZMQ 已删除）

- [x] `src/sdr/` 目录已删除
- [x] 所有引用迁移至 `zmq/data_format::ReceiveSdr`（6 子结构体 position/blood/ammo/state/gain/key）
- [x] rerun_visualizer / minimap / panels 字段路径已更新
- [x] `RadarFeed*` → `ZmqReader`/`ZmqWriter` 统一

## 工程

- [x] ZMQ 依赖集成 (zmq2 crate)
- [x] 串口/zmq 模块中文注释英文化
- [x] SerialMetadata / ZmqMetadata 监控层已删除（ZMQ 自动重连）
- [x] DJI 协议 V2.0.0 对齐（RadarMarkProcessData +4 字段，SdrEnemyRobotGainData +5 state，GAIN_DATA_LEN 36→41）
- [x] zmq_init 拆为 zmq_init_pub / zmq_init_sub
- [x] `serial_package.rs` 废弃 `robot_interaction_to_bytes`，改为 `RobotInteractionData::to_data_bytes()`
- [ ] CI/CD 构建脚本
- [ ] 测试用例补充
