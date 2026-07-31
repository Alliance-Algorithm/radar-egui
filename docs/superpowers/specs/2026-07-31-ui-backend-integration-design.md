# radar-egui UI 与后端集成设计

日期：2026-07-31

## 1. 目标

在不修改串口后端、ZMQ 后端及任何外部仓库的前提下，将现有 egui 界面与现有进程控制和共享状态正确连接，并统一当前架构、命名和文档。

本次集成覆盖：

- ROS2 Radar（`alliance_radar_location_lidar`）进程控制和 UI 状态
- Laser（`laser_guidance`）进程控制、HikCamera 说明和 UI 状态
- SDR 进程控制占位及统一阵营参数
- Serial UI 的现有共享数据展示
- Tokio 协程驱动的三组件顺序启动
- 文档中旧 Unity、错误路径、旧脚本名和错误 gRPC 状态说明的清理

## 2. 强制边界

### 2.1 允许修改

- `src/app/**`
- `src/services/**`
- `src/widgets/**`
- 必要的应用层 runtime 包装，但不能改变 Serial 或 ZMQ runtime
- `README.md`
- `AGENTS.md`
- `docs/**`
- 与上述应用层和 UI 行为直接相关的测试

### 2.2 禁止修改

- `src/serial/**`
- `src/zmq/**`
- Serial/ZMQ 的协议结构、端口、线程和现有后端测试
- `/home/yukikaze/Documents/workspace/alliance_radar_location_lidar`
- `/home/yukikaze/Documents/workspace/laser_guidance`
- `alliance_radar_sdr` 及其他外部仓库

外部仓库只允许只读检查其启动契约。

## 3. 采用方案

采用最小应用层接线方案：保留现有四个工作区和视觉语言，在 UI 与现有后端之间补齐进程编排、状态快照和展示逻辑。

不引入完整运行时重构，不为不可观察的数据伪造统计，不改变现有 Serial/ZMQ 数据流。

## 4. 运行时架构

### 4.1 总体结构

外部进程由一个独立 Tokio runtime 线程中的 worker 独占管理。egui 主线程不持有或直接并发操作 `ScriptRunner`，只发送命令并读取最新状态。

```text
egui UI
  │
  ├─ ProcessCommand (tokio::sync::mpsc)
  ▼
ProcessRuntime worker (dedicated Tokio runtime thread)
  │
  ├─ ScriptRunner / child ownership
  ├─ Start All coroutine
  ├─ tokio::time::sleep delays
  └─ stop/cancel/shutdown handling
  │
  └─ ProcessSnapshot (tokio::sync::watch)
       ▼
     egui UI
```

Serial 和 ZMQ 保持现有阻塞 `std::thread` 实现。Video 和 PointCloud 保持当前独立线程内 Tokio runtime 的实现。本次不增加全局 `#[tokio::main]`，也不让 eframe 主线程运行阻塞等待。

### 4.2 命令模型

命令保持最小且显式：

```rust
enum ProcessCommand {
    StartAll(TeamSide),
    RetryFailed,
    StartRadar(TeamSide),
    StartSdr(TeamSide),
    StartLaser {
        script: LaserScript,
        side: TeamSide,
        stream: bool,
        record: bool,
        laser_auto: bool,
    },
    StopRadar,
    StopSdr,
    StopLaser,
    StopAll,
    Shutdown,
}
```

worker 串行处理进程所有权。不得通过 `Arc<Mutex<ScriptRunner>>` 让 UI 与异步任务共同管理 child。

worker 在执行 Start All 时必须继续响应控制命令。延迟和组件启动步骤通过 `tokio::select!` 与取消信号竞争；不得让主命令接收循环因直接 `await` 整个 Start All 而无法处理 `StopAll` 或 `Shutdown`。取消只终止尚未执行的步骤，随后由 worker 串行停止已启动组件。

### 4.3 状态模型

UI 使用可复制的最新状态快照：

```rust
enum ProcessPhase {
    Idle,
    StartingRadar,
    WaitingForRadar,
    StartingSdr,
    WaitingForSdr,
    StartingLaser,
    Running,
    Failed {
        component: ProcessComponent,
        message: String,
    },
    Stopping,
}
```

快照还应包含三个组件各自的管理状态、当前全局阵营、最后错误和失败后待继续的步骤。进程管理状态与业务数据源在线状态必须分开，不能因为持有 `Child` 就声称 ZMQ、SHM 或串口数据在线。

### 4.4 Start All 协程

启动顺序固定为：

```text
ROS2 Radar → delay → SDR → delay → Laser Competition → FIFO configuration
```

延迟使用 `tokio::time::sleep`。UI 回调中不得调用 `sleep`，也不再依靠 egui 每帧比较时间点推进启动步骤。

任一步失败时：

- 停止执行后续步骤
- 保留已成功启动的组件
- 进入 `Failed` 状态并保存组件和错误
- `Retry Failed` 重试失败组件；成功后继续原 Start All 的剩余步骤
- `Stop All` 可在启动、延迟、失败或运行阶段取消后续步骤并停止三个组件

重复的 Start All 或与当前状态冲突的命令必须被明确拒绝或折叠，不能并行启动两套序列。

应用退出时发送 `Shutdown`，取消编排并清理 worker 管理的进程。

## 5. 阵营语义和参数映射

Laser 面板只保留一个全局阵营选择，表示我方颜色：

```rust
enum TeamSide {
    Red,
    Blue,
}
```

参数映射：

| 我方阵营 | ROS2 Radar | SDR | Laser |
|---|---|---|---|
| Red | `side:=red` | `--enemySide blue` | FIFO `enemy blue` |
| Blue | `side:=blue` | `--enemySide red` | FIFO `enemy red` |

单独启动和 Start All 必须使用同一映射。不得继续维护独立且可能冲突的 `radar_side` 与 `enemy_color`。

Laser 的 `Auto` 只表示 Laser 的敌方识别模式，不改变全局我方阵营，也不影响 ROS2 Radar 或 SDR 参数。未启用 Auto 时，Laser 收到全局阵营推导出的 `enemy red|blue`；启用 Auto 时，仅将 Laser FIFO 参数覆盖为 `enemy auto`。

## 6. 外部进程契约

### 6.1 ROS2 Radar

规范名称：ROS2 Radar。

仓库：

```text
/home/yukikaze/Documents/workspace/alliance_radar_location_lidar
```

解析规则：

1. 环境变量 `ALLIANCE_RADAR_LOCATION_LIDAR_ROOT`
2. 基于 `CARGO_MANIFEST_DIR` 的正确 workspace 相对位置

不得依赖进程运行时 CWD，也不得继续使用错误的 `../alliance_radar_location_lidar`。

启动前校验：

- 仓库根目录
- `ros_ws/install/setup.bash` 或对应受支持的 setup 文件
- `ros_ws/src/radar_bringup/launch/competition.launch.py`
- host map 文件（若启动命令需要显式传入）

启动契约：

```text
source /opt/ros/jazzy/setup.bash
source <repo>/ros_ws/install/setup.bash
exec ros2 launch radar_bringup competition.launch.py side:=<red|blue>
```

`competition.launch.py` 是外部真实接口名，必须保留。不得重新引入 Unity 或 `RADAR_APP`。

现有 ROS2 Radar ZMQ 通信实现保持不变。

### 6.2 Laser

仓库：

```text
/home/yukikaze/Documents/workspace/laser_guidance
```

解析规则：

1. 环境变量 `LASER_GUIDANCE_ROOT`
2. 基于 `CARGO_MANIFEST_DIR` 的正确 workspace 相对位置

脚本映射：

| UI 模式 | 脚本 |
|---|---|
| Competition | `.script/competition-laser` |
| Preview | `.script/preview-laser` |
| Stream | `.script/stream` |
| Record | `.script/record` |

Laser 当前 `.script/.config-path` 指向 HikCamera 配置。`hik.device_id` 为空时，单台设备自动选择；多台设备需要在 Laser 仓库配置中消歧。

因此：

- egui 移除 Camera 设备输入框
- egui 不传 `LASER_CAMERA_DEVICE`
- UI 显示只读 `HikCamera · managed by laser_guidance`
- 相机选择失败通过进程错误状态显示
- 不修改 Laser 仓库配置

### 6.3 SDR

保留现有 SDR 启动实现和后端。本次只将其纳入统一阵营映射、单项启停和 Start All 编排。

SDR 数据接收和页面内容可继续作为占位，不修改其 ZMQ 后端。

## 7. UI 设计

### 7.1 Laser 面板

保持当前右侧面板从上到下的单列布局：

1. 数据源
2. 比赛配置
3. Laser 脚本控制
4. 外部进程
5. 流控制

数据源区：

- 保留现有 Laser 数据源状态
- 分开显示 ZMQ 观察数据和 `/laser_frame` 视频帧状态
- 显示只读 HikCamera 配置归属
- 移除 Camera 文本输入框

比赛配置区：

- 我方红/蓝全局选择
- 同时展示参数推导结果
- `Auto` 不放入全局阵营选择

Laser 脚本区：

- 保留 Competition、Preview、Stream、Record
- 保留启动时推流和启动时内录
- 保留 Laser Auto 和 Stop Laser

外部进程区：

- ROS2 Radar、SDR、Laser 独立 Start/Stop
- Start All 文案显示 `Radar → SDR → Laser`
- 新增 `Retry Failed`
- 启动中显示当前步骤
- 失败时显示最后错误
- Stop All 在启动中和运行中都可用

不新增后端无法支撑的健康检查按钮。

### 7.2 Radar 页面

- 使用 `ROS2 Radar` 命名
- 显示进程管理状态、仓库和 launch 信息
- 沿用现有 ZMQ 已写入的 `SharedData`
- 不修改 Radar ZMQ 后端
- 明确区分 ROS2 Radar 定位数据与 `/pointcloud_frame` SHM 点云
- Rerun gRPC 只描述为可选外部可视化
- 在没有真实连接状态 API 时显示 `optional` 或 `not monitored`，不得根据 SHM 有数据声称 gRPC 已连接

### 7.3 Serial 页面

Serial UI 只读取现有 `SharedData` 和现有串口打开状态，不修改后端。

雷达标记只展示五个对方单位：

- Hero → `opponent_hero_vulnerable`
- Engineer → `opponent_engineer_vulnerable`
- Infantry 3 → `opponent_infantry_3_vulnerable`
- Infantry 4 → `opponent_infantry_4_vulnerable`
- Sentry → `opponent_sentry_vulnerable`

Aerial 和友方标记不在该五单位视图中展示。标题和图例必须与实际字段语义一致。

比赛进度：

- `elapsed / 420 s` 保持在卡片可用宽度内
- 宽度不足时允许数字换到独立一行
- 不使用会侵占相邻区域的嵌套右对齐布局

日志：

- 将“帧日志”准确命名为“状态更新日志”
- 通过比较连续 `SharedData` 快照记录可确认的状态变化
- 可记录串口打开成功/失败和 GameState、SiteEvent、RadarMarkProcess 等可观察字段变化
- 不声称是原始逐帧 HEX 日志
- 不声称可统计未由后端暴露的 CRC 失败、精确帧率或吞吐
- 相同内容的重复帧无法由快照判定，因此不计为新日志

字体：

- 普通 UI 使用现有全局 `FontFamily::Proportional`
- 状态更新日志使用现有全局 `FontFamily::Monospace`
- 不引入与现有 UI 不一致的独立字体风格

### 7.4 SDR 页面

- 保持当前 UI 和数据后端
- 纳入统一进程控制状态
- 单项 Start/Stop 和 Start All 使用全局我方阵营推导的敌方颜色
- 本次不实现新的 SDR 数据通路

## 8. 错误处理

- 仓库、脚本、setup 或 launch 文件不存在时进入 `Failed`，不得 panic
- 错误中包含组件、操作、路径和底层原因
- ROS2 环境和 Laser 脚本在 spawn 前校验
- FIFO 命令失败显示为 Laser 配置阶段错误
- UI 不因 worker 错误阻塞
- 单项启动失败不影响其他组件
- Start All 失败遵循保留已启动组件、停止后续步骤的策略
- 关闭应用时清理 worker 管理的进程

## 9. 测试设计

至少覆盖：

- `TeamSide::enemy()` 映射
- ROS2 Radar、SDR、Laser 参数映射
- ROS2 Radar 仓库解析优先级和路径校验
- Laser 仓库解析优先级和新脚本映射
- Start All 的 Radar → SDR → Laser 顺序
- 延迟期间 Stop All 取消后续步骤
- Start All 等待期间命令循环仍能响应 Stop All 和 Shutdown
- 中间步骤失败后不启动后续组件
- Retry Failed 从失败组件继续
- Serial 五个雷达标记字段映射
- Serial 状态更新日志只在可观察数据变化时追加
- 比赛进度窄宽度布局通过 UI 逻辑测试或人工预览验证

完成实现后运行：

```bash
cargo fmt --check
cargo test
cargo clippy -- -D warnings
```

还必须验证：

```bash
git diff -- src/serial src/zmq
```

该命令应为空。

## 10. 文档更新

### README.md

- 增加 Tokio command/watch channel 和 Start All 协程架构
- 写明三组件顺序和阵营参数映射
- 写明正确的 ROS2 Radar 与 Laser 仓库路径和环境变量
- 写明 Rerun gRPC 仅为可选可视化

### AGENTS.md

- 更新模块职责和 runtime 模型
- 更新启动编排和关闭流程
- 更新外部脚本名和仓库解析规则
- 强调 Serial/ZMQ 当前边界

### docs/data-flow.md

- 增加 UI → command channel → Tokio worker → external processes 流程
- 删除过时的进程轮询描述
- 修正旧 Unity、旧相对路径和旧脚本名

协议文档只纠正命名和路径，不改变 Serial/ZMQ 协议定义。

## 11. 验收标准

- 只修改 `radar-egui`
- `src/serial/**` 和 `src/zmq/**` 无 diff
- 全局我方颜色正确映射到三组件参数
- Start All 由 Tokio 协程执行，egui 主线程不阻塞
- Start All 顺序为 ROS2 Radar → SDR → Laser
- Stop All 可取消尚未执行的步骤
- Laser 使用正确仓库、新脚本和 HikCamera 配置，不显示无效 Camera 输入
- ROS2 Radar 使用正确仓库，不再作为 Unity/RADAR_APP 描述
- Serial 雷达标记正确显示五个指定对方单位
- Serial 状态更新日志不伪装成原始帧日志
- `elapsed / 420 s` 不与相邻区域重叠
- 字体与现有 UI 一致，日志使用现有等宽字体族
- README、AGENTS 和 data-flow 与实现一致
- 格式化、测试和 Clippy 全部通过
