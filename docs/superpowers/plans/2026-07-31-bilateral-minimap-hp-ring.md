# Bilateral Minimap HP Ring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show all 12 ally/enemy targets on the SDR minimap with team-colored outer rings and real enemy remaining-HP arcs, while removing the SDR occupation-status placeholders.

**Architecture:** Extend the minimap presentation model with explicit `MarkerSide`, optional health, and optional ammo. Build ally and enemy marker groups from existing `SharedData`; derive red/blue outer-ring colors from global `TeamSide`, and keep HP arc rendering independent from the team ring. No transport or backend changes are required.

**Tech Stack:** Rust 2021, egui 0.31, existing `SharedData`, existing global `TeamSide`, Rust unit tests.

## Global Constraints

- Do not modify `src/serial/**`, `src/zmq/**`, protocol fields, ports, or external repositories.
- Use existing `SharedData.ally_*`, `SharedData.enemy_*`, `sdr_blood`, and `sdr_ammo` only.
- Display exactly 12 markers: six ally and six enemy roles.
- `TeamSide` means our side: ally ring uses our red/blue color and enemy ring uses the opposite color.
- Keep role-specific center colors.
- Enemy HP arcs use real SDR blood only; ally HP and ammo are unavailable and must not be fabricated.
- Rename `Heat ring` to `HP ring` or `血量环`.
- Removing the SDR occupation UI must not remove SharedData fields or Serial SiteEvent UI.
- Preserve minimap pan, zoom, grid, labels, and marker selection.

---

### Task 1: Bilateral Markers, Team Rings, HP Arcs, and SDR Panel Cleanup

**Files:**
- Modify: `src/widgets/minimap.rs`
- Modify: `src/widgets/mod.rs`
- Modify: `src/app/sdr_workspace.rs`
- Modify: `src/widgets/panels.rs`
- Modify: `src/app/mod.rs`
- Test: `src/widgets/minimap.rs` unit tests

**Interfaces:**
- Consumes: `crate::services::script_runner::TeamSide`
- Produces: `pub enum MarkerSide { Ally, Enemy }`
- Produces: `pub struct RobotMarker { name, role_name, side, pos, role_color, team_color, health, ammo }`
- Produces: `build_robot_markers(info: &SharedData, our_side: TeamSide) -> [RobotMarker; 12]`
- Produces: `hp_arc_style(health: RobotHealth) -> Option<HpArcStyle>`

- [ ] **Step 1: Write failing marker and color-mapping tests**

Add tests in `src/widgets/minimap.rs`:

```rust
#[test]
fn red_team_builds_twelve_markers_with_red_allies_and_blue_enemies() {
    let mut data = SharedData::default();
    data.ally_hero.x = 100;
    data.enemy_hero.x = 900;
    data.sdr_blood.hero_blood = 150;
    data.sdr_ammo.hero_ammo = 42;

    let markers = build_robot_markers(&data, TeamSide::Red);
    assert_eq!(markers.len(), 12);
    assert_eq!(markers[0].side, MarkerSide::Ally);
    assert_eq!(markers[0].team_color, theme::RED);
    assert_eq!(markers[0].pos, [100, 0]);
    assert!(markers[0].health.is_none());
    assert!(markers[0].ammo.is_none());
    assert_eq!(markers[6].side, MarkerSide::Enemy);
    assert_eq!(markers[6].team_color, theme::BLUE);
    assert_eq!(markers[6].pos, [900, 0]);
    assert_eq!(markers[6].health.unwrap().hp, 150);
    assert_eq!(markers[6].ammo, Some(42));
}

#[test]
fn blue_team_swaps_ally_and_enemy_ring_colors() {
    let markers = build_robot_markers(&SharedData::default(), TeamSide::Blue);
    assert_eq!(markers[0].team_color, theme::BLUE);
    assert_eq!(markers[6].team_color, theme::RED);
}
```

- [ ] **Step 2: Run marker tests and verify they fail**

Run:

```bash
cargo test red_team_builds_twelve_markers
cargo test blue_team_swaps_ally_and_enemy_ring_colors
```

Expected: FAIL because the current builder returns only six enemy markers and has no side/team-ring model.

- [ ] **Step 3: Implement the 12-marker presentation model**

Define:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerSide { Ally, Enemy }

pub struct RobotMarker {
    pub name: &'static str,
    pub role_name: &'static str,
    pub side: MarkerSide,
    pub pos: [i16; 2],
    pub role_color: Color32,
    pub team_color: Color32,
    pub health: Option<RobotHealth>,
    pub ammo: Option<u16>,
}
```

Build ally indices `0..6` and enemy indices `6..12`, role order Hero, Engineer, Infantry 3, Infantry 4, Aerial, Sentry. Ally names use `我方 · ...`; enemy names use `敌方 · ...`. Ally health/ammo are `None`; enemy values use existing SDR fields, with Engineer ammo and Aerial health remaining `None` because no real field exists.

- [ ] **Step 4: Write failing HP arc tests**

```rust
#[test]
fn hp_arc_clamps_ratio_and_uses_green_yellow_red_thresholds() {
    assert_eq!(hp_arc_style(RobotHealth { hp: 250, hp_max: 200 }).unwrap().ratio, 1.0);
    assert_eq!(hp_arc_style(RobotHealth { hp: 150, hp_max: 200 }).unwrap().color, theme::GREEN);
    assert_eq!(hp_arc_style(RobotHealth { hp: 100, hp_max: 200 }).unwrap().color, theme::YELLOW);
    assert_eq!(hp_arc_style(RobotHealth { hp: 40, hp_max: 200 }).unwrap().color, theme::RED);
    assert!(hp_arc_style(RobotHealth { hp: 0, hp_max: 200 }).is_none());
}
```

- [ ] **Step 5: Run HP tests and verify they fail**

Run: `cargo test hp_arc_clamps_ratio_and_uses_green_yellow_red_thresholds`

Expected: FAIL because HP arc style does not exist.

- [ ] **Step 6: Implement remaining-HP arc rendering**

Create `HpArcStyle { ratio: f32, color: Color32 }`. Return `None` for zero HP or zero max; clamp ratio to `[0, 1]`; use green above 0.6, yellow above 0.3, red otherwise.

Draw the HP arc around the marker with line segments from `-FRAC_PI_2` through `ratio * TAU`. Use enough segments for a smooth arc, with a dark background ring behind it. Draw layers in this order:

```text
role center → team outer ring → optional HP background/arc → selected ring → label
```

The team ring must always remain visible. Replace `show_heat` with `show_hp_ring` throughout `MinimapOptions` and app state. UI chip text becomes `HP ring` or `血量环`.

- [ ] **Step 7: Wire TeamSide and bilateral selection into SDR workspace**

Pass `app.team_side` to both the minimap and bottom dock marker builders. Keep `sdr_selected` clamped against 12 markers. Detail UI shows marker side and coordinates. Health and ammo use `Option`; show `N/A` for ally and missing enemy fields rather than `0`.

- [ ] **Step 8: Remove SDR occupation UI only**

Delete the `StatusPanels` card titled `占领状态`. In the SDR bottom dock rename `经济 / 占领` to `经济` and delete `（无有效数据）`; retain gold values and ratio bar. Do not touch Serial SiteEvent or `SharedData` fields.

- [ ] **Step 9: Run focused and full verification**

Run:

```bash
cargo fmt --all --check
cargo test widgets::minimap
cargo test
cargo check
git diff --exit-code bf9ef3f -- src/serial src/zmq tests/runtime/serial.rs tests/runtime/zmq.rs
git diff --check bf9ef3f..HEAD
```

Expected: marker/HP tests and full suite PASS; forbidden diff empty; no whitespace errors.

- [ ] **Step 10: Commit**

```bash
git add src/widgets/minimap.rs src/widgets/mod.rs src/app/sdr_workspace.rs src/widgets/panels.rs src/app/mod.rs
git commit -m "feat: show bilateral minimap HP rings"
```

- [ ] **Step 11: Update temporary simulator and smoke test**

Do not add simulator files to git. Update `/tmp/opencode/sdr_ui_mock.py` or add a second `/tmp/opencode/lidar_ui_mock.py` so the running GUI receives dynamic enemy SDR values and ally/enemy LidarLocation values without real publishers. Rebuild and restart the feature GUI, then verify 12 moving markers, side-color swapping, HP arcs, selection details, and absence of occupation UI. Report hardware/UI limitations honestly.
