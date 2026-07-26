# Serial Workspace egui layout (from ui-mock)

Source: `docs/ui-mock/serial-workspace.html`  
Date: 2026-07-25

## Scope

**In:** mock shell proportions + monitor panels bound to `SerialData`.  
**Out:** bottom dock「发送操作 / 发送队列」(user: autonomous decision is separate).  
**Out:** real fps/CRC/throughput counters (no parser stats yet — show link state + placeholders).  
**Out:** parse switches that filter the parser (UI toggles only; do not change serial threads).

## Grid (main column after left rail 58 + right side 360)

```
topbar
hero-metrics (4 cards, full width)
stage 2-col:
  left:  比赛状态 (0x0001)
  right: 场地事件 (0x0101)
  left:  雷达标记 6×2 mark-grid (0x020C)
  right: 帧日志 (local ring buffer; open/close lines only until RX hooks)
```

No bottom TX dock — stage fills remaining height.

## Right sidebar

1. 串口连接 — Open/Close, port, baud, timeout field (timeout stored UI-only if unused by open path)
2. 解析开关 — 6 toggles, app state only
3. 小地图雷达 0x0305 — 12×(x,y) from `minimap_receive_radar_data`
4. 脏标志 — `serial_produced` / `zmq_produced` bit previews

## Data binding

| UI | Source |
|----|--------|
| 链路 Open/Closed | `serial_open` |
| 比赛状态 phase/remain/unix | `game_state_data` + `game_result_data.winner` |
| 场地事件 chips | `site_event_data` + `dart_launch_data` |
| 雷达标记 cells | `radar_mark_process_data` (红1–6 / 蓝1–6 labels; mark/vuln from ally/opponent bits) |
| 小地图表 | `minimap_receive_radar_data` |
| 脏标志 | `serial_produced` / `zmq_produced` |
| 帧日志 | local `VecDeque` in `RadarApp` (UI events); no parser hook yet |

## Files

- `src/app/serial_workspace.rs` — shell + sidebar
- `src/widgets/serial_panel.rs` — main stage cards
- `src/app/mod.rs` — optional UI state: parse toggles, serial_timeout, frame_log

## Success

Visual match to mock Serial screenshot minus TX dock; Open still wires `start_receiver`/`start_transmitter`.
