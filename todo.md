# radar-egui 开发记录

## 2026-05-05

### 项目初始化
- 创建 Cargo 项目，依赖：eframe 0.31, egui 0.31, tokio, log, env_logger
- 模块结构：main.rs, protocol.rs, tcp_client.rs, app.rs, theme.rs, widgets/

### 数据模型
- 实现 RoboMasterSignalInfo 结构体，匹配 Python SDR 的数据格式
- 实现 parse_signal() 二进制解析器，滑动窗口扫描 cmd_id

### TCP 客户端（历史实现，已由 ZMQ 替代）
- 实现 tokio 异步 TCP 客户端，连接 127.0.0.1:2000
- 支持自动重连，buffer 累积 ≥200 字节后解析

### UI 设计
- 采用 Catppuccin Mocha 配色（柔和暗色）
- 字体：JetBrainsMono NFP (英文) + LXGW WenKai (中文)
- 布局：左侧小地图 (可拖拽宽度) + 右侧状态面板

### 状态面板
- 血量：Grid 布局对齐，进度条显示
- 弹药：数值网格
- 经济：大号数值 + 进度条
- 增益：6 列表格 + 哨兵姿态

### 尝试过的方案（已回退）
- TopBottomPanel::resizable(true) - 拖拽手柄不工作
- 手动拖拽手柄 - 对齐问题
- 三面板可拖拽布局 - 用户不需要

### 当前状态
- 右侧面板固定间距 48px，不可拖拽
- 小地图可拖拽宽度
- 字体已增大，行间距已拉大

### 新增功能
- 连接配置 UI：顶栏 IP/端口输入框 + Connect 按钮
- 错误提示：连接丢失时显示红色警告
- 底部状态栏：运行时间、数据计数、目标地址、错误信息
- Connect 按钮重连逻辑：发送关闭信号、创建新通道、启动新线程
- Rerun 集成：3D 可视化机器人位置、血量/经济时间序列
- CodeRabbit 配置：PR 和 commit 自动 review

## SDR 接口（2026-05 历史记录，非当前架构）
- ✅ 127.0.0.1:2000 — 信号流 (102 bytes) — 已对接
- ❌ 127.0.0.1:3000 — 噪声流 (7 bytes) — 未对接
- ❌ 192.168.1.10:2000 — 数据中心标记 (12 bytes) — 未对接
- ❌ 192.168.1.10:3000 — 数据中心发送 — 未对接

## 2026-05-18

### 进程控制优化（历史实现，后由 ProcessRuntime actor 取代）
- 当时将 Laser 延迟启动放到 update loop；当前已由 Tokio `ProcessRuntime` actor 的 `tokio::select!` 可取消编排取代
- Stop 按钮可靠停止 daemon：pkill -9 强杀 tool_competition/tool_preview/ffplay，清理 FIFO
- Laser UDP listener 改为懒启动：仅在进入 Laser 标签时绑定 5001，避免冷启动端口冲突

### UI 改进
- 主题切换改为全局左下角 pill + 自绘 sun/moon 图标，支持滑动动画
- Laser 数据源状态拆分为 Listening（端口已绑定）和 Receiving（收到数据包）
- 浅色模式下月亮图标加深，提高可读性

### 视频流
- 共享内存消费者增加 800ms 无帧超时重连，解决 stop/restart 后画面不更新
- 断开时清空旧帧，UI 正确回落 "等待视频流..."

### 部署
- deploy.sh autostart 配置修复（DEPLOY_ROOT 路径）
- 检测阈值从 0.25 调至 0.35，减少虚警

## 2026-07-10

### 文档与代码清理
- 移除 `TransmitRadarSync` ZMQ PUB 通路（结构体、常量、发送逻辑、标志位）
- 新增强化 `docs/data-flow.md`：精简数据流文档，整合雷达激光定位链路
- 新增 `docs/lidar-location-protocol.md`：LidarLocation ZMQ/JSON 协议文档
- 更新 README：移除 radar_sync 引用，新增 LidarLocation 数据源

## 2026-07-24

### 文档对齐：Radar 进程 = alliance_radar_location_lidar（非 Unity）
- 代码侧 `ScriptRunner::start_radar` 启动 ROS2 `radar_bringup competition.launch.py`；当前默认 root 为 manifest 相对 `../../alliance_radar_location_lidar`，可用 `ALLIANCE_RADAR_LOCATION_LIDAR_ROOT` 覆盖
- Unity/RADAR_APP 仅为已废弃历史方案，不得重新作为当前启动目标
- 更新 README、docs/data-flow.md 进程控制与数据流说明
- 重写 `AGENTS.md`：去掉 TCP:2000 / tcp_client / protocol.rs，改为 ZMQ + ROS2 Radar + 当前模块/启动编排

## 2026-07-31

### UI/backend integration
- 进程控制改为 egui → Tokio `mpsc<ProcessCommand>` → `ProcessRuntime` actor → `ScriptRunner`，状态由 `watch<ProcessSnapshot>` 返回 UI
- Start All 当前顺序为 Radar → SDR → Competition Laser → FIFO；`tokio::select!` 保证启动间隔和重试期间可取消
- 全局我方 `TeamSide` 统一派生 Radar 我方 side、SDR 敌方 side 和 Laser enemy 命令
- Laser 当前脚本为 `competition-laser`、`preview-laser`、`stream`、`record`；HikCamera 由 `laser_guidance` 管理

## 待办
- [ ] 测试 Rerun 集成
- [ ] 添加噪声流接口 (127.0.0.1:3000)
- [ ] 添加数据中心接口 (192.168.1.10:2000)
- [ ] 添加数据导出功能
