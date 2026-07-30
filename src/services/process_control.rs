use super::process_runtime::{
    ProcessCommand, ProcessRuntime, ProcessSendError, ProcessSnapshot, StartAllOptions,
    StartLaserOptions,
};
use super::script_runner::{self, LaserScript, TeamSide};

pub enum TeamSideInput<'a> {
    Side(TeamSide),
    Legacy(&'a str),
}

impl From<TeamSide> for TeamSideInput<'_> {
    fn from(side: TeamSide) -> Self {
        Self::Side(side)
    }
}

impl<'a> From<&'a str> for TeamSideInput<'a> {
    fn from(side: &'a str) -> Self {
        Self::Legacy(side)
    }
}

impl<'a> From<&'a String> for TeamSideInput<'a> {
    fn from(side: &'a String) -> Self {
        Self::Legacy(side.as_str())
    }
}

pub struct ProcessControl {
    runtime: ProcessRuntime,
}

impl ProcessControl {
    pub fn new() -> Self {
        Self {
            runtime: ProcessRuntime::start(),
        }
    }

    pub fn snapshot(&self) -> ProcessSnapshot {
        self.runtime.snapshot()
    }

    pub fn start_all(&self, options: StartAllOptions) -> Result<(), ProcessSendError> {
        self.runtime.send(options.into())
    }

    pub fn retry_failed(&self) -> Result<(), ProcessSendError> {
        self.runtime.send(ProcessCommand::RetryFailed)
    }

    pub fn start_radar<'a>(
        &self,
        side: impl Into<TeamSideInput<'a>>,
    ) -> Result<(), ProcessSendError> {
        let side = match side.into() {
            TeamSideInput::Side(side) => side,
            TeamSideInput::Legacy("blue") => TeamSide::Blue,
            TeamSideInput::Legacy(_) => TeamSide::Red,
        };
        self.runtime.send(ProcessCommand::StartRadar(side))
    }

    pub fn start_sdr<'a>(
        &self,
        side: impl Into<TeamSideInput<'a>>,
    ) -> Result<(), ProcessSendError> {
        let side = match side.into() {
            TeamSideInput::Side(side) => side,
            TeamSideInput::Legacy("red") => TeamSide::Blue,
            TeamSideInput::Legacy(_) => TeamSide::Red,
        };
        self.runtime.send(ProcessCommand::StartSdr(side))
    }

    pub fn start_laser(&self, options: StartLaserOptions) -> Result<(), ProcessSendError> {
        self.runtime.send(ProcessCommand::StartLaser(options))
    }

    pub fn stop_all(&self) -> Result<(), ProcessSendError> {
        self.runtime.send(ProcessCommand::StopAll)
    }

    pub fn is_running(&self) -> bool {
        self.snapshot().laser.managed
    }

    pub fn active(&self) -> Option<LaserScript> {
        self.snapshot().laser.active_laser
    }

    pub fn daemon_alive(&self) -> bool {
        script_runner::daemon_alive()
    }

    pub fn is_sdr_running(&self) -> bool {
        self.snapshot().sdr.managed
    }

    pub fn is_radar_running(&self) -> bool {
        self.snapshot().radar.managed
    }

    pub fn has_pending_start_all(&self) -> bool {
        matches!(
            self.snapshot().phase,
            super::process_runtime::ProcessPhase::StartingRadar
                | super::process_runtime::ProcessPhase::WaitingForRadar
                | super::process_runtime::ProcessPhase::StartingSdr
                | super::process_runtime::ProcessPhase::WaitingForSdr
                | super::process_runtime::ProcessPhase::StartingLaser
                | super::process_runtime::ProcessPhase::ConfiguringLaser
        )
    }

    pub fn start_script(
        &self,
        script: LaserScript,
        _camera_device: &str,
    ) -> Result<(), ProcessSendError> {
        self.start_laser(StartLaserOptions {
            script,
            side: TeamSide::Red,
            stream: false,
            record: false,
            laser_auto: false,
            configure: false,
        })
    }

    pub fn start_script_with_daemon_config(
        &self,
        script: LaserScript,
        _camera_device: &str,
        enemy_cmd: String,
        stream_cmd: String,
        record_cmd: String,
    ) -> Result<(), ProcessSendError> {
        self.start_laser(StartLaserOptions {
            script,
            side: if enemy_cmd == "enemy red" {
                TeamSide::Blue
            } else {
                TeamSide::Red
            },
            stream: stream_cmd == "stream on",
            record: record_cmd == "record on",
            laser_auto: enemy_cmd == "enemy auto",
            configure: script.is_daemon(),
        })
    }

    pub fn stop_script(&self) {
        let _ = self.runtime.send(ProcessCommand::StopLaser);
    }

    pub fn stop_sdr(&self) {
        let _ = self.runtime.send(ProcessCommand::StopSdr);
    }

    pub fn stop_radar(&self) {
        let _ = self.runtime.send(ProcessCommand::StopRadar);
    }

    pub fn schedule_start_all(
        &self,
        sdr_enemy_color: &str,
        _camera_device: &str,
        enemy_cmd: String,
        stream_cmd: String,
        record_cmd: String,
    ) -> Result<(), ProcessSendError> {
        let side = if sdr_enemy_color == "red" {
            TeamSide::Blue
        } else {
            TeamSide::Red
        };
        self.start_all(StartAllOptions {
            side,
            stream: stream_cmd == "stream on",
            record: record_cmd == "record on",
            laser_auto: enemy_cmd == "enemy auto",
        })
    }

    pub fn send_laser_command(&self, command: &str) {
        let _ = self
            .runtime
            .send(ProcessCommand::SendLaserCommand(command.to_owned()));
    }
}

impl Default for ProcessControl {
    fn default() -> Self {
        Self::new()
    }
}
