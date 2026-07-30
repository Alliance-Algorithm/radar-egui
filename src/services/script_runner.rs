//! 比赛组件进程管理模块
//!
//! 从 radar-egui 中 spawn 比赛所需的三个外部进程：
//!   - ROS2 Radar       (alliance_radar_location_lidar: camera + lidar + fusion + bridge)
//!   - laser_guidance  脚本 (competition-laser / preview-laser / stream / record)
//!   - SDR 数据桥接    (alliance_radar_sdr/thread_init.py)

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

const RADAR_ROOT_ENV: &str = "ALLIANCE_RADAR_LOCATION_LIDAR_ROOT";
const LASER_ROOT_ENV: &str = "LASER_GUIDANCE_ROOT";
const LASER_FIFO: &str = "/tmp/laser_cmd";
const SDR_REPO: &str = "../alliance_radar_sdr";
const RADAR_STDERR_LOG: &str = "/tmp/radar-egui-radar.stderr.log";
const SDR_STDERR_LOG: &str = "/tmp/radar-egui-sdr.stderr.log";
const LASER_STDERR_LOG: &str = "/tmp/radar-egui-laser.stderr.log";

pub(crate) struct ProcessExit {
    pub component: super::process_runtime::ProcessComponent,
    pub detail: String,
}

// ── LaserScript ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TeamSide {
    #[default]
    Red,
    Blue,
}

impl TeamSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Blue => "blue",
        }
    }

    pub fn enemy(self) -> Self {
        match self {
            Self::Red => Self::Blue,
            Self::Blue => Self::Red,
        }
    }

    pub fn laser_enemy_command(self, laser_auto: bool) -> &'static str {
        if laser_auto {
            "enemy auto"
        } else {
            match self.enemy() {
                Self::Red => "enemy red",
                Self::Blue => "enemy blue",
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum LaserScript {
    Competition,
    Preview,
    Stream,
    Record,
}

impl LaserScript {
    pub fn label(&self) -> &'static str {
        match self {
            LaserScript::Competition => "Competition",
            LaserScript::Preview => "Preview",
            LaserScript::Stream => "Stream",
            LaserScript::Record => "Record",
        }
    }

    pub fn script_name(self) -> &'static str {
        match self {
            LaserScript::Competition => "competition-laser",
            LaserScript::Preview => "preview-laser",
            LaserScript::Stream => "stream",
            LaserScript::Record => "record",
        }
    }

    pub fn is_daemon(&self) -> bool {
        matches!(self, LaserScript::Competition | LaserScript::Stream)
    }
}

// ── ScriptRunner ─────────────────────────────────────────────────────────────

pub struct ScriptRunner {
    // ROS2 Radar
    radar_child: Option<Child>,

    // Laser
    child: Option<Child>,
    active: Option<LaserScript>,

    // SDR bridge
    sdr_child: Option<Child>,
}

impl ScriptRunner {
    pub fn new() -> Self {
        Self {
            radar_child: None,
            child: None,
            active: None,
            sdr_child: None,
        }
    }

    // ── ROS2 Radar ────────────────────────────────────────────────────

    pub fn start_radar(&mut self, side: &str) -> io::Result<()> {
        self.stop_radar();

        let repo = resolve_radar_root().map_err(|error| {
            contextual_error(error, "Radar", "resolve workspace", &radar_root_candidate())
        })?;
        let cmd = format!(
            "source /opt/ros/jazzy/setup.bash && \
             source ros_ws/install/setup.bash && \
             exec ros2 launch radar_bringup competition.launch.py side:={side}"
        );

        let stderr = stderr_log(RADAR_STDERR_LOG, "Radar")?;
        let child = Command::new("bash")
            .args(["-lc", &cmd])
            .current_dir(&repo)
            .stdout(Stdio::null())
            .stderr(stderr)
            .stdin(Stdio::null())
            .spawn()
            .map_err(|error| contextual_error(error, "Radar", "spawn bash launch", &repo))?;

        log::info!("Started Radar (side={side}, pid={})", child.id());
        self.radar_child = Some(child);
        Ok(())
    }

    pub fn stop_radar(&mut self) {
        if let Some(mut child) = self.radar_child.take() {
            let _ = child.kill();
            let _ = child.wait();
            log::info!("Stopped Radar");
        }
    }

    pub fn is_radar_running(&self) -> bool {
        self.radar_child.is_some()
    }

    // ── Laser ────────────────────────────────────────────────────────────────

    pub fn start(&mut self, script: LaserScript) -> io::Result<()> {
        self.stop();

        let laser_root = resolve_laser_root().map_err(|error| {
            contextual_error(
                error,
                "Laser",
                "resolve script root",
                &laser_root_candidate(),
            )
        })?;
        let path = laser_root.join(".script").join(script.script_name());
        let stderr = stderr_log(LASER_STDERR_LOG, "Laser")?;
        let child = Command::new(&path)
            .current_dir(&laser_root)
            .env("LASER_HEADLESS", "1")
            .stdout(Stdio::null())
            .stderr(stderr)
            .stdin(Stdio::null())
            .spawn()
            .map_err(|error| contextual_error(error, "Laser", "spawn script", &path))?;

        log::info!(
            "Started laser script: {:?} from {} (pid={})",
            script,
            laser_root.display(),
            child.id()
        );
        self.child = Some(child);
        self.active = Some(script);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(active) = self.active {
            if active.is_daemon() {
                // 1. 优雅退出：通过 FIFO 通知 daemon
                send_fifo("quit").ok();
                std::thread::sleep(std::time::Duration::from_millis(800));
                // 2. 兜底强杀 (SIGKILL)：daemon 被 disown，wrapper kill 无效
                for name in &["tool_competition", "tool_preview", "ffplay"] {
                    let _ = Command::new("pkill").args(["-9", "-f", name]).output();
                }
                // 3. 清理 FIFO，避免残留阻塞下次启动
                let _ = std::fs::remove_file(LASER_FIFO);
            }
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
            log::info!("Stopped laser script wrapper");
        }
        self.active = None;
    }

    pub fn is_running(&self) -> bool {
        self.active.is_some()
    }

    pub fn active(&self) -> Option<LaserScript> {
        self.active
    }

    // ── SDR ──────────────────────────────────────────────────────────────────

    /// 启动 SDR 数据桥接 (thread_init.py)
    ///
    /// 从 SDR 仓库根目录运行，PYTHONPATH=. 解决 parser/tcp 导入。
    pub fn start_sdr(&mut self, enemy_color: &str) -> io::Result<()> {
        self.stop_sdr();

        let sdr_dir = std::path::absolute(SDR_REPO).map_err(|error| {
            contextual_error(error, "SDR", "resolve repository", Path::new(SDR_REPO))
        })?;
        let script = sdr_dir.join("thread_init.py");
        let stderr = stderr_log(SDR_STDERR_LOG, "SDR")?;
        let child = Command::new("python3")
            .args(["thread_init.py", "--enemySide", enemy_color])
            .current_dir(&sdr_dir)
            .env("PYTHONPATH", ".")
            .stdout(Stdio::null())
            .stderr(stderr)
            .stdin(Stdio::null())
            .spawn()
            .map_err(|error| contextual_error(error, "SDR", "spawn thread_init.py", &script))?;

        log::info!(
            "Started SDR bridge (pid={}) with enemy={enemy_color}",
            child.id()
        );
        self.sdr_child = Some(child);
        Ok(())
    }

    pub fn stop_sdr(&mut self) {
        if let Some(mut child) = self.sdr_child.take() {
            let _ = child.kill();
            let _ = child.wait();
            log::info!("Stopped SDR bridge");
        }
    }

    pub fn is_sdr_running(&self) -> bool {
        self.sdr_child.is_some()
    }

    pub fn stop_all(&mut self) {
        self.stop_radar();
        self.stop();
        self.stop_sdr();
    }

    pub(crate) fn poll_exits(&mut self) -> Vec<ProcessExit> {
        let mut exits = Vec::new();
        poll_child(
            &mut self.radar_child,
            super::process_runtime::ProcessComponent::Radar,
            RADAR_STDERR_LOG,
            &mut exits,
        );
        poll_child(
            &mut self.sdr_child,
            super::process_runtime::ProcessComponent::Sdr,
            SDR_STDERR_LOG,
            &mut exits,
        );
        let laser_exited = poll_child(
            &mut self.child,
            super::process_runtime::ProcessComponent::Laser,
            LASER_STDERR_LOG,
            &mut exits,
        );
        if laser_exited {
            self.active = None;
        }
        exits
    }
}

impl Drop for ScriptRunner {
    fn drop(&mut self) {
        self.stop_all();
    }
}

// ── 静态辅助函数 ────────────────────────────────────────────────────────────

pub fn resolve_radar_root() -> io::Result<PathBuf> {
    if let Some(root) = std::env::var_os(RADAR_ROOT_ENV) {
        return valid_radar_root(PathBuf::from(root));
    }

    valid_radar_root(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../alliance_radar_location_lidar"),
    )
}

fn radar_root_candidate() -> PathBuf {
    std::env::var_os(RADAR_ROOT_ENV).map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../alliance_radar_location_lidar"),
        PathBuf::from,
    )
}

pub fn resolve_laser_root() -> io::Result<PathBuf> {
    if let Some(root) = std::env::var_os(LASER_ROOT_ENV) {
        return valid_laser_root(PathBuf::from(root));
    }

    valid_laser_root(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../laser_guidance"))
}

fn laser_root_candidate() -> PathBuf {
    std::env::var_os(LASER_ROOT_ENV).map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../laser_guidance"),
        PathBuf::from,
    )
}

fn contextual_error(error: io::Error, component: &str, operation: &str, path: &Path) -> io::Error {
    io::Error::new(
        error.kind(),
        format!(
            "{component} {operation} at {} failed: {error}",
            path.display()
        ),
    )
}

fn stderr_log(path: &str, component: &str) -> io::Result<Stdio> {
    std::fs::File::create(path)
        .map(Stdio::from)
        .map_err(|error| contextual_error(error, component, "open stderr log", Path::new(path)))
}

fn poll_child(
    child: &mut Option<Child>,
    component: super::process_runtime::ProcessComponent,
    log_path: &str,
    exits: &mut Vec<ProcessExit>,
) -> bool {
    let Some(process) = child.as_mut() else {
        return false;
    };
    let detail = match process.try_wait() {
        Ok(Some(status)) => Some(format!("{status}; stderr: {log_path}")),
        Ok(None) => None,
        Err(error) => {
            let _ = process.kill();
            let _ = process.wait();
            Some(format!(
                "status poll failed and process was terminated: {error}; stderr: {log_path}"
            ))
        }
    };
    if let Some(detail) = detail {
        *child = None;
        exits.push(ProcessExit { component, detail });
        true
    } else {
        false
    }
}

fn valid_radar_root(path: PathBuf) -> io::Result<PathBuf> {
    const REQUIRED: [&str; 2] = [
        "ros_ws/install/setup.bash",
        "ros_ws/src/radar_bringup/launch/competition.launch.py",
    ];
    valid_root(path, &REQUIRED, "Radar workspace")
}

fn valid_laser_root(path: PathBuf) -> io::Result<PathBuf> {
    const REQUIRED: [&str; 4] = [
        ".script/competition-laser",
        ".script/preview-laser",
        ".script/stream",
        ".script/record",
    ];
    valid_root(path, &REQUIRED, "laser_guidance scripts")
}

fn valid_root(path: PathBuf, required: &[&str], contract: &str) -> io::Result<PathBuf> {
    let absolute = std::path::absolute(path)?;
    let missing: Vec<_> = required
        .iter()
        .copied()
        .filter(|relative| !absolute.join(relative).is_file())
        .collect();
    if missing.is_empty() {
        absolute.canonicalize()
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "{} does not satisfy {contract}; missing {}",
                absolute.display(),
                missing.join(", ")
            ),
        ))
    }
}

pub fn daemon_alive() -> bool {
    use std::os::unix::fs::FileTypeExt;
    match std::fs::metadata(LASER_FIFO) {
        Ok(meta) => meta.file_type().is_fifo(),
        Err(_) => false,
    }
}

pub(crate) fn laser_fifo_path() -> &'static Path {
    Path::new(LASER_FIFO)
}

pub fn send_fifo(cmd: &str) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut opts = std::fs::OpenOptions::new();
    opts.write(true);
    opts.custom_flags(libc::O_NONBLOCK);

    let mut fifo = opts.open(LASER_FIFO)?;
    writeln!(fifo, "{cmd}")?;
    log::info!("FIFO sent: {}", cmd);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_test_dir(name: &str) -> PathBuf {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "radar-egui-{name}-{}-{timestamp}",
            std::process::id()
        ))
    }

    #[test]
    fn team_side_maps_our_and_enemy_colors() {
        assert_eq!(TeamSide::Red.as_str(), "red");
        assert_eq!(TeamSide::Red.enemy(), TeamSide::Blue);
        assert_eq!(TeamSide::Blue.enemy(), TeamSide::Red);
        assert_eq!(TeamSide::Red.laser_enemy_command(false), "enemy blue");
        assert_eq!(TeamSide::Blue.laser_enemy_command(false), "enemy red");
        assert_eq!(TeamSide::Red.laser_enemy_command(true), "enemy auto");
    }

    #[test]
    fn laser_scripts_match_current_repository_contract() {
        assert_eq!(LaserScript::Competition.script_name(), "competition-laser");
        assert_eq!(LaserScript::Preview.script_name(), "preview-laser");
        assert_eq!(LaserScript::Stream.script_name(), "stream");
        assert_eq!(LaserScript::Record.script_name(), "record");
    }

    #[test]
    fn valid_laser_root_requires_current_scripts() {
        let temp = temp_test_dir("laser-root");
        std::fs::create_dir_all(temp.join(".script")).unwrap();
        std::fs::write(temp.join(".script/competition-laser"), "").unwrap();
        std::fs::write(temp.join(".script/preview-laser"), "").unwrap();
        std::fs::write(temp.join(".script/stream"), "").unwrap();
        std::fs::write(temp.join(".script/record"), "").unwrap();
        assert!(valid_laser_root(temp.clone()).is_ok());
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn valid_radar_root_requires_workspace_contract() {
        let temp = temp_test_dir("radar-root");
        std::fs::create_dir_all(temp.join("ros_ws/install")).unwrap();
        std::fs::create_dir_all(temp.join("ros_ws/src/radar_bringup/launch")).unwrap();
        std::fs::write(temp.join("ros_ws/install/setup.bash"), "").unwrap();
        std::fs::write(
            temp.join("ros_ws/src/radar_bringup/launch/competition.launch.py"),
            "",
        )
        .unwrap();
        assert!(valid_radar_root(temp.clone()).is_ok());
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn valid_laser_root_rejects_incomplete_script_contract() {
        let temp = temp_test_dir("incomplete-laser-root");
        std::fs::create_dir_all(temp.join(".script")).unwrap();
        std::fs::write(temp.join(".script/competition-laser"), "").unwrap();

        let error = valid_laser_root(temp.clone()).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(error.to_string().contains("preview-laser"));
        assert!(error.to_string().contains("stream"));
        assert!(error.to_string().contains("record"));
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn valid_radar_root_rejects_incomplete_workspace_contract() {
        let temp = temp_test_dir("incomplete-radar-root");
        std::fs::create_dir_all(temp.join("ros_ws/install")).unwrap();
        std::fs::write(temp.join("ros_ws/install/setup.bash"), "").unwrap();

        let error = valid_radar_root(temp.clone()).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(error.to_string().contains("competition.launch.py"));
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn contextual_errors_include_component_operation_path_and_cause() {
        let error = contextual_error(
            io::Error::new(io::ErrorKind::PermissionDenied, "denied by kernel"),
            "Laser",
            "spawn script",
            Path::new("/tmp/laser script"),
        );

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(
            error.to_string(),
            "Laser spawn script at /tmp/laser script failed: denied by kernel"
        );
    }

    #[test]
    fn test_labels() {
        assert_eq!(LaserScript::Competition.label(), "Competition");
        assert_eq!(LaserScript::Preview.label(), "Preview");
        assert_eq!(LaserScript::Stream.label(), "Stream");
        assert_eq!(LaserScript::Record.label(), "Record");
    }

    #[test]
    fn test_is_daemon() {
        assert!(LaserScript::Competition.is_daemon());
        assert!(!LaserScript::Preview.is_daemon());
        assert!(LaserScript::Stream.is_daemon());
        assert!(!LaserScript::Record.is_daemon());
    }

    #[test]
    fn test_new_runner_is_idle() {
        let runner = ScriptRunner::new();
        assert!(!runner.is_running());
        assert!(!runner.is_sdr_running());
        assert!(runner.active().is_none());
    }
}
