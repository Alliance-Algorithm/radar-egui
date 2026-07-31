# 双方目标小地图与血量环设计

日期：2026-07-31

## 目标

SDR 小地图同时显示敌我双方目标，通过红蓝阵营外圈区分双方，并将现有 `Heat ring` 改为准确的剩余血量圆弧。

## 约束

- 不修改 `src/zmq/**`、`src/serial/**`、协议、端口或外部仓库。
- 只读取现有 `SharedData.enemy_*`、`SharedData.ally_*`、`sdr_blood` 和 `sdr_ammo`。
- 不伪造我方血量或弹药。
- 沿用 Laser 面板中的全局 `TeamSide`，其含义是我方阵营。
- 保持现有小地图缩放、拖动、标签、网格和点击选择行为。
- 移除 SDR 页面中的“占领状态”展示和“经济 / 占领”占位文案；保留真实经济数据。
- 不删除 `SharedData` 中的占领字段，也不影响 Serial 页真实的裁判场地事件展示。

## 目标集合

小地图构建 12 个 marker：

- 我方：Hero、Engineer、Infantry 3、Infantry 4、Aerial、Sentry
- 敌方：Hero、Engineer、Infantry 3、Infantry 4、Aerial、Sentry

双方位置分别读取：

- 我方：`SharedData.ally_*`
- 敌方：`SharedData.enemy_*`

当前数据来源保持不变：SDR 更新敌方位置和敌方状态，ROS2 Radar LidarLocation 可更新敌我双方位置。若不同数据源写入同一字段，仍遵循现有 SharedData 的最后写入值；本功能不改变融合策略。

## 阵营映射

每个 marker 绘制独立的阵营外圈：

| 全局我方阵营 | ally 外圈 | enemy 外圈 |
|---|---|---|
| Red | 红色 | 蓝色 |
| Blue | 蓝色 | 红色 |

阵营外圈始终显示，不受血量环开关影响。

Marker 中心颜色继续按角色区分，避免只凭阵营颜色丢失角色识别能力。

## 血量环

现有 `Heat ring` 重命名为 `HP ring`，中文界面使用 `血量环`。

血量环仅用于有真实血量数据的敌方目标：

```text
ratio = clamp(current_hp / max_hp, 0, 1)
arc = ratio × 360°
```

显示规则：

- `ratio > 0.6`：绿色
- `0.3 < ratio <= 0.6`：黄色
- `ratio <= 0.3`：红色
- `ratio == 1`：完整圆环
- `ratio == 0`：不绘制剩余血量弧，但 marker 和阵营外圈仍显示

当前最大血量沿用现有 UI 值：Hero、Engineer、Infantry 3、Infantry 4 为 200，Sentry 为 400。Aerial 当前无 blood 字段，因此不绘制血量环。

我方当前没有对应血量来源，不绘制血量环，也不以 0 血量显示。

关闭 `血量环` 时只隐藏血量弧，阵营外圈、角色中心和选择圈继续显示。

## 选择与详情

点击任意 marker 后：

- 底部详情显示阵营、角色和坐标。
- 敌方有真实 SDR 数据时显示血量和弹药。
- 我方血量和弹药显示 `N/A`，不显示 0 或模拟值。
- 列表名称包含阵营前缀，例如 `我方 · 英雄`、`敌方 · 英雄`。

选择索引适配 12 个 marker，并在数据切换或布局变化时保持安全边界。

## SDR 状态面板

- 删除 `StatusPanels` 中的“占领状态 / 点位控制概览”卡片。
- 底部第三列从“经济 / 占领”改为“经济”。
- 删除该列中的“（无有效数据）”占领占位文案。
- 保留 `remaining_gold / total_gold` 数值和进度条。
- 本次不改变 SDR 消息中的占领字段，也不改变 Serial 工作区的 SiteEvent 卡片。

## 临时模拟数据

产品代码不内置模拟发布器。手工 UI 测试使用 `/tmp/opencode` 下的一次性 ZMQ 发布器：

- `:5555` 发布 SDR 敌方位置、血量、弹药和状态。
- `:5556` 可发布 LidarLocation 双方位置以预览 12 个目标。
- 模拟器不得提交到仓库。
- 测试时不得与真实 SDR/Radar 发布器同时绑定相同端口。

## 测试

单元测试至少覆盖：

- Red/Blue 两种我方阵营的 ally/enemy 外圈映射。
- 12 个 marker 的角色、阵营和字段映射。
- 敌方血量和弹药映射。
- 我方 health/ammo 为不可用。
- HP ratio clamp 和绿/黄/红阈值。
- 12 marker 选择索引边界。

验证命令：

```bash
cargo fmt --all --check
cargo test
git diff --exit-code -- src/serial src/zmq tests/runtime/serial.rs tests/runtime/zmq.rs
```

## 验收标准

- 小地图同时显示 12 个双方目标。
- 阵营外圈随全局我方阵营自动红蓝互换。
- 血量环显示敌方真实剩余血量，颜色和圆弧长度正确。
- UI 不再使用 `Heat ring` 命名。
- 我方没有虚假血量、弹药或血量环。
- 点击双方目标时详情语义准确。
- SDR 页面不再显示“占领状态”卡片或占领占位文案，经济数据继续显示。
- Serial/ZMQ 后端和外部仓库无修改。
