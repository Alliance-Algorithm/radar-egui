use std::fmt;
use std::io;
use std::thread::JoinHandle;
use std::time::Duration;

use tokio::sync::{mpsc, watch};

use super::script_runner::{self, LaserScript, ScriptRunner, TeamSide};

const START_ALL_DELAY: Duration = Duration::from_secs(1);
const FIFO_RETRY_DELAY: Duration = Duration::from_millis(50);
const FIFO_ATTEMPTS: u8 = 100;
const DAEMON_PROBE_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessComponent {
    Radar,
    Sdr,
    Laser,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProcessPhase {
    #[default]
    Idle,
    StartingRadar,
    WaitingForRadar,
    StartingSdr,
    WaitingForSdr,
    StartingLaser,
    ConfiguringLaser,
    Running,
    Stopping,
    Failed(ProcessComponent),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartAllOptions {
    pub side: TeamSide,
    pub stream: bool,
    pub record: bool,
    pub laser_auto: bool,
    pub radar_record: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StartLaserOptions {
    pub script: LaserScript,
    pub side: TeamSide,
    pub stream: bool,
    pub record: bool,
    pub laser_auto: bool,
    pub configure: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProcessCommand {
    StartAll {
        side: TeamSide,
        stream: bool,
        record: bool,
        laser_auto: bool,
        radar_record: bool,
    },
    RetryFailed,
    StartRadar {
        side: TeamSide,
        record: bool,
    },
    StartSdr(TeamSide),
    StartLaser(StartLaserOptions),
    SendLaserCommand(String),
    StopRadar,
    StopSdr,
    StopLaser,
    StopAll,
    Shutdown,
}

impl From<StartAllOptions> for ProcessCommand {
    fn from(options: StartAllOptions) -> Self {
        Self::StartAll {
            side: options.side,
            stream: options.stream,
            record: options.record,
            laser_auto: options.laser_auto,
            radar_record: options.radar_record,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComponentSnapshot {
    pub managed: bool,
    pub active_laser: Option<LaserScript>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProcessSnapshot {
    pub phase: ProcessPhase,
    pub radar: ComponentSnapshot,
    pub sdr: ComponentSnapshot,
    pub laser: ComponentSnapshot,
    pub daemon_available: bool,
    pub error: Option<String>,
}

pub(crate) struct ComponentExit {
    component: ProcessComponent,
    detail: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessSendError;

impl fmt::Display for ProcessSendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("process runtime is not available")
    }
}

impl std::error::Error for ProcessSendError {}

pub(crate) trait ProcessBackend: Send + 'static {
    fn start_radar(&mut self, side: TeamSide, record: bool) -> io::Result<()>;
    fn start_sdr(&mut self, enemy: TeamSide) -> io::Result<()>;
    fn start_laser(&mut self, script: LaserScript) -> io::Result<()>;
    fn configure_laser(&mut self, enemy: &str, stream: bool, record: bool) -> io::Result<()>;
    fn stop_radar(&mut self);
    fn stop_sdr(&mut self);
    fn stop_laser(&mut self);
    fn daemon_alive(&mut self) -> bool;
    fn poll_exits(&mut self) -> Vec<ComponentExit>;
}

impl ProcessBackend for ScriptRunner {
    fn start_radar(&mut self, side: TeamSide, record: bool) -> io::Result<()> {
        ScriptRunner::start_radar(self, side.as_str(), record).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("Radar start failed for side {}: {error}", side.as_str()),
            )
        })
    }

    fn start_sdr(&mut self, enemy: TeamSide) -> io::Result<()> {
        ScriptRunner::start_sdr(self, enemy.as_str()).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("SDR start failed for enemy {}: {error}", enemy.as_str()),
            )
        })
    }

    fn start_laser(&mut self, script: LaserScript) -> io::Result<()> {
        ScriptRunner::start(self, script).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "Laser start failed for script {}: {error}",
                    script.script_name()
                ),
            )
        })
    }

    fn configure_laser(&mut self, enemy: &str, stream: bool, record: bool) -> io::Result<()> {
        for command in [
            enemy,
            if stream { "stream on" } else { "stream off" },
            if record { "record on" } else { "record off" },
        ] {
            script_runner::send_fifo(command).map_err(|error| {
                io::Error::new(
                    error.kind(),
                    format!(
                        "Laser FIFO configuration command {command:?} at {} failed: {error}",
                        script_runner::laser_fifo_path().display()
                    ),
                )
            })?;
        }
        Ok(())
    }

    fn stop_radar(&mut self) {
        ScriptRunner::stop_radar(self);
    }

    fn stop_sdr(&mut self) {
        ScriptRunner::stop_sdr(self);
    }

    fn stop_laser(&mut self) {
        ScriptRunner::stop(self);
    }

    fn daemon_alive(&mut self) -> bool {
        script_runner::daemon_alive()
    }

    fn poll_exits(&mut self) -> Vec<ComponentExit> {
        ScriptRunner::poll_exits(self)
            .into_iter()
            .map(|exit| ComponentExit {
                component: exit.component,
                detail: exit.detail,
            })
            .collect()
    }
}

#[derive(Clone, Copy, Debug)]
enum SequenceStep {
    StartRadar,
    WaitAfterRadar,
    StartSdr,
    WaitAfterSdr,
    StartLaser,
    ConfigureLaser,
}

#[derive(Clone, Copy, Debug)]
struct Sequence {
    command: StartAllOptions,
    step: SequenceStep,
    configure_attempts: u8,
}

#[derive(Clone, Copy, Debug)]
struct FailedSequence {
    command: StartAllOptions,
    step: SequenceStep,
}

#[derive(Clone, Copy, Debug)]
struct LaserConfiguration {
    enemy: &'static str,
    stream: bool,
    record: bool,
    attempts: u8,
}

pub struct ProcessRuntime {
    command_tx: Option<mpsc::UnboundedSender<ProcessCommand>>,
    snapshot_rx: watch::Receiver<ProcessSnapshot>,
    thread: Option<JoinHandle<()>>,
}

impl ProcessRuntime {
    pub fn start() -> Self {
        Self::start_with_backend(ScriptRunner::new(), START_ALL_DELAY)
    }

    fn start_with_backend<B: ProcessBackend>(backend: B, delay: Duration) -> Self {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (snapshot_tx, snapshot_rx) = watch::channel(ProcessSnapshot::default());
        let thread = std::thread::spawn(move || {
            tokio::runtime::Runtime::new()
                .expect("process runtime")
                .block_on(run_process_actor(backend, command_rx, snapshot_tx, delay));
        });
        Self {
            command_tx: Some(command_tx),
            snapshot_rx,
            thread: Some(thread),
        }
    }

    pub fn send(&self, command: ProcessCommand) -> Result<(), ProcessSendError> {
        self.command_tx
            .as_ref()
            .ok_or(ProcessSendError)?
            .send(command)
            .map_err(|_| ProcessSendError)
    }

    pub fn snapshot(&self) -> ProcessSnapshot {
        self.snapshot_rx.borrow().clone()
    }
}

impl Drop for ProcessRuntime {
    fn drop(&mut self) {
        if let Some(command_tx) = self.command_tx.take() {
            let _ = command_tx.send(ProcessCommand::Shutdown);
        }
        if let Some(thread) = self.thread.take() {
            if thread.thread().id() != std::thread::current().id() {
                let _ = thread.join();
            }
        }
    }
}

async fn run_process_actor<B: ProcessBackend>(
    mut backend: B,
    mut command_rx: mpsc::UnboundedReceiver<ProcessCommand>,
    snapshot_tx: watch::Sender<ProcessSnapshot>,
    delay: Duration,
) {
    let mut snapshot = ProcessSnapshot::default();
    let mut sequence: Option<Sequence> = None;
    let mut failed: Option<FailedSequence> = None;
    let mut laser_configuration: Option<LaserConfiguration> = None;
    let mut deadline: Option<tokio::time::Instant> = None;
    let mut daemon_probe = tokio::time::interval(DAEMON_PROBE_INTERVAL);

    loop {
        let command = if let Some(at) = deadline {
            tokio::select! {
                command = command_rx.recv() => command,
                _ = daemon_probe.tick() => {
                    reconcile_process_exits(
                        &mut backend,
                        &mut snapshot,
                        &snapshot_tx,
                        &mut sequence,
                        &mut failed,
                        &mut laser_configuration,
                        &mut deadline,
                    );
                    publish_daemon_availability(&mut backend, &mut snapshot, &snapshot_tx);
                    continue;
                }
                () = tokio::time::sleep_until(at) => {
                    deadline = None;
                    if sequence.is_some() {
                        advance_sequence(
                            &mut backend,
                            &mut snapshot,
                            &snapshot_tx,
                            &mut sequence,
                            &mut failed,
                            &mut deadline,
                            delay,
                        );
                    } else {
                        configure_standalone_laser(
                            &mut backend,
                            &mut snapshot,
                            &snapshot_tx,
                            &mut laser_configuration,
                            &mut deadline,
                        );
                    }
                    continue;
                }
            }
        } else {
            tokio::select! {
                command = command_rx.recv() => command,
                _ = daemon_probe.tick() => {
                    reconcile_process_exits(
                        &mut backend,
                        &mut snapshot,
                        &snapshot_tx,
                        &mut sequence,
                        &mut failed,
                        &mut laser_configuration,
                        &mut deadline,
                    );
                    publish_daemon_availability(&mut backend, &mut snapshot, &snapshot_tx);
                    continue;
                }
            }
        };

        let Some(command) = command else {
            stop_all(&mut backend, &mut snapshot, &snapshot_tx);
            break;
        };
        match command {
            ProcessCommand::StartAll {
                side,
                stream,
                record,
                laser_auto,
                radar_record,
            } => {
                if reject_start_if_busy(
                    "Start All",
                    &sequence,
                    &laser_configuration,
                    &mut snapshot,
                    &snapshot_tx,
                ) {
                    continue;
                }
                reset_pending(
                    &mut sequence,
                    &mut failed,
                    &mut laser_configuration,
                    &mut deadline,
                );
                sequence = Some(Sequence {
                    command: StartAllOptions {
                        side,
                        stream,
                        record,
                        laser_auto,
                        radar_record,
                    },
                    step: SequenceStep::StartRadar,
                    configure_attempts: 0,
                });
                advance_sequence(
                    &mut backend,
                    &mut snapshot,
                    &snapshot_tx,
                    &mut sequence,
                    &mut failed,
                    &mut deadline,
                    delay,
                );
            }
            ProcessCommand::RetryFailed => {
                if let Some(failure) = failed.take() {
                    sequence = Some(Sequence {
                        command: failure.command,
                        step: failure.step,
                        configure_attempts: 0,
                    });
                    advance_sequence(
                        &mut backend,
                        &mut snapshot,
                        &snapshot_tx,
                        &mut sequence,
                        &mut failed,
                        &mut deadline,
                        delay,
                    );
                }
            }
            ProcessCommand::StartRadar { side, record } => {
                if reject_start_if_busy(
                    "Radar start",
                    &sequence,
                    &laser_configuration,
                    &mut snapshot,
                    &snapshot_tx,
                ) {
                    continue;
                }
                reset_pending(
                    &mut sequence,
                    &mut failed,
                    &mut laser_configuration,
                    &mut deadline,
                );
                snapshot.phase = ProcessPhase::StartingRadar;
                snapshot.error = None;
                if snapshot.radar.managed {
                    backend.stop_radar();
                }
                snapshot.radar = ComponentSnapshot::default();
                publish(&snapshot_tx, &snapshot);
                finish_component(
                    backend.start_radar(side, record),
                    ProcessComponent::Radar,
                    &mut snapshot,
                    &snapshot_tx,
                );
            }
            ProcessCommand::StartSdr(side) => {
                if reject_start_if_busy(
                    "SDR start",
                    &sequence,
                    &laser_configuration,
                    &mut snapshot,
                    &snapshot_tx,
                ) {
                    continue;
                }
                reset_pending(
                    &mut sequence,
                    &mut failed,
                    &mut laser_configuration,
                    &mut deadline,
                );
                snapshot.phase = ProcessPhase::StartingSdr;
                snapshot.error = None;
                if snapshot.sdr.managed {
                    backend.stop_sdr();
                }
                snapshot.sdr = ComponentSnapshot::default();
                publish(&snapshot_tx, &snapshot);
                finish_component(
                    backend.start_sdr(side.enemy()),
                    ProcessComponent::Sdr,
                    &mut snapshot,
                    &snapshot_tx,
                );
            }
            ProcessCommand::StartLaser(options) => {
                if reject_start_if_busy(
                    "Laser start",
                    &sequence,
                    &laser_configuration,
                    &mut snapshot,
                    &snapshot_tx,
                ) {
                    continue;
                }
                reset_pending(
                    &mut sequence,
                    &mut failed,
                    &mut laser_configuration,
                    &mut deadline,
                );
                snapshot.phase = ProcessPhase::StartingLaser;
                snapshot.error = None;
                if snapshot.laser.managed {
                    backend.stop_laser();
                }
                snapshot.laser = ComponentSnapshot::default();
                publish(&snapshot_tx, &snapshot);
                match backend.start_laser(options.script) {
                    Ok(()) => {
                        snapshot.laser.managed = true;
                        snapshot.laser.active_laser = Some(options.script);
                        if options.configure {
                            laser_configuration = Some(LaserConfiguration {
                                enemy: options.side.laser_enemy_command(options.laser_auto),
                                stream: options.stream,
                                record: options.record,
                                attempts: 0,
                            });
                            configure_standalone_laser(
                                &mut backend,
                                &mut snapshot,
                                &snapshot_tx,
                                &mut laser_configuration,
                                &mut deadline,
                            );
                        } else {
                            snapshot.phase = ProcessPhase::Running;
                            publish(&snapshot_tx, &snapshot);
                        }
                    }
                    Err(error) => {
                        fail_component(error, ProcessComponent::Laser, &mut snapshot, &snapshot_tx)
                    }
                }
            }
            ProcessCommand::SendLaserCommand(command) => {
                if let Err(error) = script_runner::send_fifo(&command) {
                    snapshot.error = Some(format!(
                        "Laser FIFO command {command:?} at {} failed: {error}",
                        script_runner::laser_fifo_path().display()
                    ));
                    publish(&snapshot_tx, &snapshot);
                }
            }
            ProcessCommand::StopRadar => {
                reset_pending(
                    &mut sequence,
                    &mut failed,
                    &mut laser_configuration,
                    &mut deadline,
                );
                backend.stop_radar();
                snapshot.radar = ComponentSnapshot::default();
                snapshot.phase = managed_phase(&snapshot);
                publish(&snapshot_tx, &snapshot);
            }
            ProcessCommand::StopSdr => {
                reset_pending(
                    &mut sequence,
                    &mut failed,
                    &mut laser_configuration,
                    &mut deadline,
                );
                backend.stop_sdr();
                snapshot.sdr = ComponentSnapshot::default();
                snapshot.phase = managed_phase(&snapshot);
                publish(&snapshot_tx, &snapshot);
            }
            ProcessCommand::StopLaser => {
                reset_pending(
                    &mut sequence,
                    &mut failed,
                    &mut laser_configuration,
                    &mut deadline,
                );
                backend.stop_laser();
                snapshot.laser = ComponentSnapshot::default();
                snapshot.phase = managed_phase(&snapshot);
                publish(&snapshot_tx, &snapshot);
            }
            ProcessCommand::StopAll => {
                reset_pending(
                    &mut sequence,
                    &mut failed,
                    &mut laser_configuration,
                    &mut deadline,
                );
                stop_all(&mut backend, &mut snapshot, &snapshot_tx);
            }
            ProcessCommand::Shutdown => {
                stop_all(&mut backend, &mut snapshot, &snapshot_tx);
                break;
            }
        }
    }
}

fn reject_start_if_busy(
    operation: &str,
    sequence: &Option<Sequence>,
    laser_configuration: &Option<LaserConfiguration>,
    snapshot: &mut ProcessSnapshot,
    snapshot_tx: &watch::Sender<ProcessSnapshot>,
) -> bool {
    if sequence.is_none() && laser_configuration.is_none() {
        return false;
    }
    snapshot.error = Some(format!(
        "{operation} rejected while process phase {:?} is active; current operation continues",
        snapshot.phase
    ));
    publish(snapshot_tx, snapshot);
    true
}

fn managed_phase(snapshot: &ProcessSnapshot) -> ProcessPhase {
    if snapshot.radar.managed || snapshot.sdr.managed || snapshot.laser.managed {
        ProcessPhase::Running
    } else {
        ProcessPhase::Idle
    }
}

fn reconcile_process_exits<B: ProcessBackend>(
    backend: &mut B,
    snapshot: &mut ProcessSnapshot,
    snapshot_tx: &watch::Sender<ProcessSnapshot>,
    sequence: &mut Option<Sequence>,
    failed: &mut Option<FailedSequence>,
    laser_configuration: &mut Option<LaserConfiguration>,
    deadline: &mut Option<tokio::time::Instant>,
) {
    let exits = backend.poll_exits();
    if !exits.is_empty() {
        reset_pending(sequence, failed, laser_configuration, deadline);
    }
    for exit in exits {
        let component_snapshot = match exit.component {
            ProcessComponent::Radar => &mut snapshot.radar,
            ProcessComponent::Sdr => &mut snapshot.sdr,
            ProcessComponent::Laser => &mut snapshot.laser,
        };
        component_snapshot.managed = false;
        component_snapshot.active_laser = None;
        snapshot.phase = ProcessPhase::Failed(exit.component);
        snapshot.error = Some(format!(
            "{:?} process exited: {}",
            exit.component, exit.detail
        ));
        publish(snapshot_tx, snapshot);
    }
}

fn reset_pending(
    sequence: &mut Option<Sequence>,
    failed: &mut Option<FailedSequence>,
    laser_configuration: &mut Option<LaserConfiguration>,
    deadline: &mut Option<tokio::time::Instant>,
) {
    *sequence = None;
    *failed = None;
    *laser_configuration = None;
    *deadline = None;
}

fn publish_daemon_availability<B: ProcessBackend>(
    backend: &mut B,
    snapshot: &mut ProcessSnapshot,
    snapshot_tx: &watch::Sender<ProcessSnapshot>,
) {
    let available = backend.daemon_alive();
    if snapshot.daemon_available != available {
        snapshot.daemon_available = available;
        publish(snapshot_tx, snapshot);
    }
}

fn configure_standalone_laser<B: ProcessBackend>(
    backend: &mut B,
    snapshot: &mut ProcessSnapshot,
    snapshot_tx: &watch::Sender<ProcessSnapshot>,
    configuration: &mut Option<LaserConfiguration>,
    deadline: &mut Option<tokio::time::Instant>,
) {
    let Some(mut current) = configuration.take() else {
        return;
    };
    snapshot.phase = ProcessPhase::ConfiguringLaser;
    publish(snapshot_tx, snapshot);
    match backend.configure_laser(current.enemy, current.stream, current.record) {
        Ok(()) => {
            snapshot.phase = ProcessPhase::Running;
            snapshot.error = None;
            publish(snapshot_tx, snapshot);
        }
        Err(error) if current.attempts + 1 < FIFO_ATTEMPTS => {
            current.attempts += 1;
            *configuration = Some(current);
            *deadline = Some(tokio::time::Instant::now() + FIFO_RETRY_DELAY);
            snapshot.error = Some(error.to_string());
            publish(snapshot_tx, snapshot);
        }
        Err(error) => fail_component(error, ProcessComponent::Laser, snapshot, snapshot_tx),
    }
}

fn advance_sequence<B: ProcessBackend>(
    backend: &mut B,
    snapshot: &mut ProcessSnapshot,
    snapshot_tx: &watch::Sender<ProcessSnapshot>,
    sequence: &mut Option<Sequence>,
    failed: &mut Option<FailedSequence>,
    deadline: &mut Option<tokio::time::Instant>,
    delay: Duration,
) {
    let Some(mut current) = sequence.take() else {
        return;
    };
    match current.step {
        SequenceStep::StartRadar => {
            snapshot.phase = ProcessPhase::StartingRadar;
            snapshot.error = None;
            if snapshot.radar.managed {
                backend.stop_radar();
            }
            snapshot.radar = ComponentSnapshot::default();
            publish(snapshot_tx, snapshot);
            match backend.start_radar(current.command.side, current.command.radar_record) {
                Ok(()) => {
                    snapshot.radar.managed = true;
                    snapshot.phase = ProcessPhase::WaitingForRadar;
                    current.step = SequenceStep::WaitAfterRadar;
                    *deadline = Some(tokio::time::Instant::now() + delay);
                    *sequence = Some(current);
                    publish(snapshot_tx, snapshot);
                }
                Err(error) => sequence_failed(
                    error,
                    current,
                    ProcessComponent::Radar,
                    snapshot,
                    snapshot_tx,
                    failed,
                ),
            }
        }
        SequenceStep::WaitAfterRadar => {
            current.step = SequenceStep::StartSdr;
            *sequence = Some(current);
            advance_sequence(
                backend,
                snapshot,
                snapshot_tx,
                sequence,
                failed,
                deadline,
                delay,
            );
        }
        SequenceStep::StartSdr => {
            snapshot.phase = ProcessPhase::StartingSdr;
            if snapshot.sdr.managed {
                backend.stop_sdr();
            }
            snapshot.sdr = ComponentSnapshot::default();
            publish(snapshot_tx, snapshot);
            match backend.start_sdr(current.command.side.enemy()) {
                Ok(()) => {
                    snapshot.sdr.managed = true;
                    snapshot.phase = ProcessPhase::WaitingForSdr;
                    current.step = SequenceStep::WaitAfterSdr;
                    *deadline = Some(tokio::time::Instant::now() + delay);
                    *sequence = Some(current);
                    publish(snapshot_tx, snapshot);
                }
                Err(error) => sequence_failed(
                    error,
                    current,
                    ProcessComponent::Sdr,
                    snapshot,
                    snapshot_tx,
                    failed,
                ),
            }
        }
        SequenceStep::WaitAfterSdr => {
            current.step = SequenceStep::StartLaser;
            *sequence = Some(current);
            advance_sequence(
                backend,
                snapshot,
                snapshot_tx,
                sequence,
                failed,
                deadline,
                delay,
            );
        }
        SequenceStep::StartLaser => {
            snapshot.phase = ProcessPhase::StartingLaser;
            if snapshot.laser.managed {
                backend.stop_laser();
            }
            snapshot.laser = ComponentSnapshot::default();
            publish(snapshot_tx, snapshot);
            match backend.start_laser(LaserScript::Competition) {
                Ok(()) => {
                    snapshot.laser.managed = true;
                    snapshot.laser.active_laser = Some(LaserScript::Competition);
                    current.step = SequenceStep::ConfigureLaser;
                    *sequence = Some(current);
                    advance_sequence(
                        backend,
                        snapshot,
                        snapshot_tx,
                        sequence,
                        failed,
                        deadline,
                        delay,
                    );
                }
                Err(error) => sequence_failed(
                    error,
                    current,
                    ProcessComponent::Laser,
                    snapshot,
                    snapshot_tx,
                    failed,
                ),
            }
        }
        SequenceStep::ConfigureLaser => {
            snapshot.phase = ProcessPhase::ConfiguringLaser;
            publish(snapshot_tx, snapshot);
            let enemy = current
                .command
                .side
                .laser_enemy_command(current.command.laser_auto);
            match backend.configure_laser(enemy, current.command.stream, current.command.record) {
                Ok(()) => {
                    snapshot.phase = ProcessPhase::Running;
                    snapshot.error = None;
                    publish(snapshot_tx, snapshot);
                }
                Err(error) if current.configure_attempts + 1 < FIFO_ATTEMPTS => {
                    current.configure_attempts += 1;
                    *deadline = Some(tokio::time::Instant::now() + FIFO_RETRY_DELAY);
                    *sequence = Some(current);
                    snapshot.error = Some(error.to_string());
                    publish(snapshot_tx, snapshot);
                }
                Err(error) => sequence_failed(
                    error,
                    current,
                    ProcessComponent::Laser,
                    snapshot,
                    snapshot_tx,
                    failed,
                ),
            }
        }
    }
}

fn sequence_failed(
    error: io::Error,
    sequence: Sequence,
    component: ProcessComponent,
    snapshot: &mut ProcessSnapshot,
    snapshot_tx: &watch::Sender<ProcessSnapshot>,
    failed: &mut Option<FailedSequence>,
) {
    *failed = Some(FailedSequence {
        command: sequence.command,
        step: sequence.step,
    });
    fail_component(error, component, snapshot, snapshot_tx);
}

fn finish_component(
    result: io::Result<()>,
    component: ProcessComponent,
    snapshot: &mut ProcessSnapshot,
    snapshot_tx: &watch::Sender<ProcessSnapshot>,
) {
    match result {
        Ok(()) => {
            let component_snapshot = match component {
                ProcessComponent::Radar => &mut snapshot.radar,
                ProcessComponent::Sdr => &mut snapshot.sdr,
                ProcessComponent::Laser => &mut snapshot.laser,
            };
            component_snapshot.managed = true;
            snapshot.phase = ProcessPhase::Running;
            snapshot.error = None;
            publish(snapshot_tx, snapshot);
        }
        Err(error) => fail_component(error, component, snapshot, snapshot_tx),
    }
}

fn fail_component(
    error: io::Error,
    component: ProcessComponent,
    snapshot: &mut ProcessSnapshot,
    snapshot_tx: &watch::Sender<ProcessSnapshot>,
) {
    snapshot.phase = ProcessPhase::Failed(component);
    snapshot.error = Some(error.to_string());
    publish(snapshot_tx, snapshot);
}

fn stop_all<B: ProcessBackend>(
    backend: &mut B,
    snapshot: &mut ProcessSnapshot,
    snapshot_tx: &watch::Sender<ProcessSnapshot>,
) {
    let daemon_available = snapshot.daemon_available;
    snapshot.phase = ProcessPhase::Stopping;
    snapshot.error = None;
    publish(snapshot_tx, snapshot);
    backend.stop_laser();
    backend.stop_sdr();
    backend.stop_radar();
    *snapshot = ProcessSnapshot {
        daemon_available,
        ..ProcessSnapshot::default()
    };
    publish(snapshot_tx, snapshot);
}

fn publish(snapshot_tx: &watch::Sender<ProcessSnapshot>, snapshot: &ProcessSnapshot) {
    let _ = snapshot_tx.send(snapshot.clone());
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::*;
    use crate::services::script_runner::{LaserScript, TeamSide};

    #[derive(Default)]
    struct FakeBackend {
        events: Arc<Mutex<Vec<String>>>,
        fail_once: Option<ProcessComponent>,
        fail_on_attempt: Option<(ProcessComponent, u8)>,
        start_attempts: [u8; 3],
        configure_failures: u8,
        daemon_available: bool,
        exited: Option<ProcessComponent>,
    }

    impl FakeBackend {
        fn fail_once(component: ProcessComponent) -> Self {
            Self {
                fail_once: Some(component),
                ..Self::default()
            }
        }

        fn fail_configure_once() -> Self {
            Self {
                configure_failures: 1,
                ..Self::default()
            }
        }

        fn fail_on_attempt(component: ProcessComponent, attempt: u8) -> Self {
            Self {
                fail_on_attempt: Some((component, attempt)),
                ..Self::default()
            }
        }

        fn with_daemon_available() -> Self {
            Self {
                daemon_available: true,
                ..Self::default()
            }
        }

        fn event(&mut self, component: ProcessComponent, event: String) -> io::Result<()> {
            self.events.lock().unwrap().push(event);
            let index = match component {
                ProcessComponent::Radar => 0,
                ProcessComponent::Sdr => 1,
                ProcessComponent::Laser => 2,
            };
            self.start_attempts[index] += 1;
            if self.fail_once == Some(component) {
                self.fail_once = None;
                Err(io::Error::other("configured failure"))
            } else if self.fail_on_attempt == Some((component, self.start_attempts[index])) {
                Err(io::Error::other("configured replacement failure"))
            } else {
                Ok(())
            }
        }
    }

    impl ProcessBackend for FakeBackend {
        fn start_radar(&mut self, side: TeamSide, record: bool) -> io::Result<()> {
            self.event(
                ProcessComponent::Radar,
                format!(
                    "radar:{},record {}",
                    side.as_str(),
                    if record { "on" } else { "off" }
                ),
            )
        }

        fn start_sdr(&mut self, enemy: TeamSide) -> io::Result<()> {
            self.event(ProcessComponent::Sdr, format!("sdr:{}", enemy.as_str()))
        }

        fn start_laser(&mut self, script: LaserScript) -> io::Result<()> {
            self.event(ProcessComponent::Laser, format!("laser:{}", script.label()))
        }

        fn configure_laser(&mut self, enemy: &str, stream: bool, record: bool) -> io::Result<()> {
            let result = self.event(
                ProcessComponent::Laser,
                format!(
                    "fifo:{enemy},stream {},record {}",
                    if stream { "on" } else { "off" },
                    if record { "on" } else { "off" }
                ),
            );
            if result.is_ok() && self.configure_failures > 0 {
                self.configure_failures -= 1;
                Err(io::Error::other("configured FIFO failure"))
            } else {
                result
            }
        }

        fn stop_radar(&mut self) {
            self.events.lock().unwrap().push("stop:radar".into());
        }

        fn stop_sdr(&mut self) {
            self.events.lock().unwrap().push("stop:sdr".into());
        }

        fn stop_laser(&mut self) {
            self.events.lock().unwrap().push("stop:laser".into());
        }

        fn daemon_alive(&mut self) -> bool {
            self.daemon_available
        }

        fn poll_exits(&mut self) -> Vec<ComponentExit> {
            let started = self.exited.is_some_and(|component| {
                let index = match component {
                    ProcessComponent::Radar => 0,
                    ProcessComponent::Sdr => 1,
                    ProcessComponent::Laser => 2,
                };
                self.start_attempts[index] > 0
            });
            started
                .then(|| self.exited.take())
                .flatten()
                .map(|component| ComponentExit {
                    component,
                    detail: "exit status: 17".into(),
                })
                .into_iter()
                .collect()
        }
    }

    fn start_all_red() -> ProcessCommand {
        ProcessCommand::StartAll {
            side: TeamSide::Red,
            stream: true,
            record: false,
            laser_auto: false,
            radar_record: false,
        }
    }

    fn test_runtime(
        backend: FakeBackend,
        delay: Duration,
    ) -> (ProcessRuntime, Arc<Mutex<Vec<String>>>) {
        let events = Arc::clone(&backend.events);
        (ProcessRuntime::start_with_backend(backend, delay), events)
    }

    async fn wait_for_phase(runtime: &ProcessRuntime, phase: ProcessPhase, timeout: Duration) {
        tokio::time::timeout(timeout, async {
            loop {
                if runtime.snapshot().phase == phase {
                    return;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("timed out waiting for process phase");
    }

    async fn wait_for_failed(
        runtime: &ProcessRuntime,
        component: ProcessComponent,
        timeout: Duration,
    ) {
        wait_for_phase(runtime, ProcessPhase::Failed(component), timeout).await;
    }

    #[tokio::test]
    async fn start_all_runs_radar_then_sdr_then_laser() {
        let delay = Duration::from_millis(1);
        let (runtime, events) = test_runtime(FakeBackend::default(), delay);
        runtime.send(start_all_red()).unwrap();

        wait_for_phase(&runtime, ProcessPhase::Running, Duration::from_secs(1)).await;

        assert_eq!(
            events.lock().unwrap().as_slice(),
            [
                "radar:red,record off",
                "sdr:blue",
                "laser:Competition",
                "fifo:enemy blue,stream on,record off",
            ]
        );
    }

    #[tokio::test]
    async fn start_all_propagates_radar_record_flag() {
        let delay = Duration::from_millis(1);
        let (runtime, events) = test_runtime(FakeBackend::default(), delay);
        runtime
            .send(ProcessCommand::StartAll {
                side: TeamSide::Blue,
                stream: false,
                record: false,
                laser_auto: false,
                radar_record: true,
            })
            .unwrap();

        wait_for_phase(&runtime, ProcessPhase::Running, Duration::from_secs(1)).await;

        assert!(events
            .lock()
            .unwrap()
            .starts_with(&["radar:blue,record on".into()]));
    }

    #[tokio::test]
    async fn repeated_start_all_during_delay_is_rejected_without_cancelling_sequence() {
        let delay = Duration::from_millis(30);
        let (runtime, events) = test_runtime(FakeBackend::default(), delay);
        runtime.send(start_all_red()).unwrap();
        wait_for_phase(
            &runtime,
            ProcessPhase::WaitingForRadar,
            Duration::from_secs(1),
        )
        .await;

        runtime.send(start_all_red()).unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            while !runtime.snapshot().error.as_deref().is_some_and(|error| {
                error.contains("Start All rejected") && error.contains("WaitingForRadar")
            }) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        wait_for_phase(&runtime, ProcessPhase::Running, Duration::from_secs(1)).await;

        assert_eq!(
            events
                .lock()
                .unwrap()
                .iter()
                .filter(|event| *event == "radar:red,record off")
                .count(),
            1
        );
        assert!(events
            .lock()
            .unwrap()
            .iter()
            .any(|event| event.starts_with("fifo:")));
    }

    #[tokio::test]
    async fn component_start_during_delay_is_rejected_without_cancelling_sequence() {
        let delay = Duration::from_millis(30);
        let (runtime, events) = test_runtime(FakeBackend::default(), delay);
        runtime.send(start_all_red()).unwrap();
        wait_for_phase(
            &runtime,
            ProcessPhase::WaitingForRadar,
            Duration::from_secs(1),
        )
        .await;

        runtime
            .send(ProcessCommand::StartSdr(TeamSide::Blue))
            .unwrap();
        wait_for_phase(&runtime, ProcessPhase::Running, Duration::from_secs(1)).await;

        assert_eq!(
            events
                .lock()
                .unwrap()
                .iter()
                .filter(|event| event.starts_with("sdr:"))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn component_start_during_fifo_retry_is_rejected_without_cancelling_retry() {
        let (runtime, events) = test_runtime(FakeBackend::fail_configure_once(), Duration::ZERO);
        runtime.send(start_all_red()).unwrap();
        wait_for_phase(
            &runtime,
            ProcessPhase::ConfiguringLaser,
            Duration::from_secs(1),
        )
        .await;

        runtime
            .send(ProcessCommand::StartRadar {
                side: TeamSide::Blue,
                record: false,
            })
            .unwrap();
        wait_for_phase(&runtime, ProcessPhase::Running, Duration::from_secs(1)).await;

        assert_eq!(
            events
                .lock()
                .unwrap()
                .iter()
                .filter(|event| event.starts_with("radar:"))
                .count(),
            1
        );
        assert_eq!(
            events
                .lock()
                .unwrap()
                .iter()
                .filter(|event| event.starts_with("fifo:"))
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn standalone_start_radar_propagates_record_flag() {
        let (runtime, events) = test_runtime(FakeBackend::default(), Duration::ZERO);
        runtime
            .send(ProcessCommand::StartRadar {
                side: TeamSide::Blue,
                record: true,
            })
            .unwrap();
        wait_for_phase(&runtime, ProcessPhase::Running, Duration::from_secs(1)).await;

        assert_eq!(events.lock().unwrap().as_slice(), ["radar:blue,record on"]);
    }

    #[tokio::test]
    async fn failure_stops_later_steps_and_retry_continues() {
        let delay = Duration::from_millis(1);
        let backend = FakeBackend::fail_once(ProcessComponent::Sdr);
        let (runtime, events) = test_runtime(backend, delay);
        runtime.send(start_all_red()).unwrap();
        wait_for_failed(&runtime, ProcessComponent::Sdr, Duration::from_secs(1)).await;
        assert_eq!(
            events.lock().unwrap().as_slice(),
            ["radar:red,record off", "sdr:blue"]
        );

        runtime.send(ProcessCommand::RetryFailed).unwrap();
        wait_for_phase(&runtime, ProcessPhase::Running, Duration::from_secs(1)).await;
        assert!(events.lock().unwrap().ends_with(&[
            "sdr:blue".into(),
            "laser:Competition".into(),
            "fifo:enemy blue,stream on,record off".into(),
        ]));
    }

    #[tokio::test]
    async fn stop_all_during_delay_cancels_remaining_steps() {
        let delay = Duration::from_secs(30);
        let (runtime, events) = test_runtime(FakeBackend::default(), delay);
        runtime.send(start_all_red()).unwrap();
        wait_for_phase(
            &runtime,
            ProcessPhase::WaitingForRadar,
            Duration::from_secs(1),
        )
        .await;
        runtime.send(ProcessCommand::StopAll).unwrap();
        wait_for_phase(&runtime, ProcessPhase::Idle, Duration::from_secs(1)).await;

        assert!(!events
            .lock()
            .unwrap()
            .iter()
            .any(|event| event.starts_with("sdr:")));
        assert_eq!(runtime.snapshot().phase, ProcessPhase::Idle);
    }

    #[tokio::test]
    async fn standalone_laser_configuration_retries_without_blocking_the_actor() {
        let (runtime, events) = test_runtime(FakeBackend::fail_configure_once(), Duration::ZERO);
        runtime
            .send(ProcessCommand::StartLaser(StartLaserOptions {
                script: LaserScript::Competition,
                side: TeamSide::Red,
                stream: true,
                record: false,
                laser_auto: false,
                configure: true,
            }))
            .unwrap();

        wait_for_phase(&runtime, ProcessPhase::Running, Duration::from_secs(1)).await;

        assert_eq!(
            events.lock().unwrap().as_slice(),
            [
                "laser:Competition",
                "fifo:enemy blue,stream on,record off",
                "fifo:enemy blue,stream on,record off",
            ]
        );
    }

    #[tokio::test]
    async fn daemon_availability_is_published_by_the_actor() {
        let (runtime, _) = test_runtime(FakeBackend::with_daemon_available(), Duration::ZERO);

        tokio::time::timeout(Duration::from_secs(1), async {
            while !runtime.snapshot().daemon_available {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("timed out waiting for daemon availability");
    }

    #[tokio::test]
    async fn failed_laser_replacement_clears_stale_managed_state() {
        let backend = FakeBackend::fail_on_attempt(ProcessComponent::Laser, 2);
        let (runtime, _) = test_runtime(backend, Duration::ZERO);
        let options = StartLaserOptions {
            script: LaserScript::Competition,
            side: TeamSide::Red,
            stream: false,
            record: false,
            laser_auto: false,
            configure: false,
        };
        runtime.send(ProcessCommand::StartLaser(options)).unwrap();
        wait_for_phase(&runtime, ProcessPhase::Running, Duration::from_secs(1)).await;

        runtime.send(ProcessCommand::StartLaser(options)).unwrap();
        wait_for_failed(&runtime, ProcessComponent::Laser, Duration::from_secs(1)).await;

        assert_eq!(runtime.snapshot().laser, ComponentSnapshot::default());
    }

    #[tokio::test]
    async fn replacing_managed_laser_stops_old_process_before_starting_new_one() {
        let (runtime, events) = test_runtime(FakeBackend::default(), Duration::ZERO);
        let options = StartLaserOptions {
            script: LaserScript::Preview,
            side: TeamSide::Red,
            stream: false,
            record: false,
            laser_auto: false,
            configure: false,
        };
        runtime.send(ProcessCommand::StartLaser(options)).unwrap();
        wait_for_phase(&runtime, ProcessPhase::Running, Duration::from_secs(1)).await;
        runtime.send(ProcessCommand::StartLaser(options)).unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            while events.lock().unwrap().len() < 3 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();

        assert_eq!(
            events.lock().unwrap().as_slice(),
            ["laser:Preview", "stop:laser", "laser:Preview"]
        );
    }

    #[tokio::test]
    async fn child_exit_clears_managed_state_and_publishes_context() {
        let backend = FakeBackend {
            exited: Some(ProcessComponent::Radar),
            ..FakeBackend::default()
        };
        let (runtime, _) = test_runtime(backend, Duration::ZERO);
        runtime
            .send(ProcessCommand::StartRadar {
                side: TeamSide::Red,
                record: false,
            })
            .unwrap();
        wait_for_failed(&runtime, ProcessComponent::Radar, Duration::from_secs(1)).await;

        let snapshot = runtime.snapshot();
        assert!(!snapshot.radar.managed);
        assert!(snapshot
            .error
            .unwrap()
            .contains("Radar process exited: exit status: 17"));
    }

    #[tokio::test]
    async fn stopping_one_component_keeps_running_phase_while_others_are_managed() {
        for (command, expected) in [
            (ProcessCommand::StopRadar, [false, true, true]),
            (ProcessCommand::StopSdr, [true, false, true]),
            (ProcessCommand::StopLaser, [true, true, false]),
        ] {
            let (runtime, _) = test_runtime(FakeBackend::default(), Duration::ZERO);
            runtime.send(start_all_red()).unwrap();
            wait_for_phase(&runtime, ProcessPhase::Running, Duration::from_secs(1)).await;
            runtime.send(command).unwrap();
            tokio::time::timeout(Duration::from_secs(1), async {
                loop {
                    let snapshot = runtime.snapshot();
                    if [
                        snapshot.radar.managed,
                        snapshot.sdr.managed,
                        snapshot.laser.managed,
                    ] == expected
                    {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("timed out waiting for component stop");
            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.phase, ProcessPhase::Running);
            assert_eq!(
                [
                    snapshot.radar.managed,
                    snapshot.sdr.managed,
                    snapshot.laser.managed,
                ],
                expected
            );
        }
    }

    #[tokio::test]
    async fn shutdown_interrupts_start_all_wait_and_joins_promptly() {
        let (runtime, events) = test_runtime(FakeBackend::default(), Duration::from_secs(30));
        runtime.send(start_all_red()).unwrap();
        wait_for_phase(
            &runtime,
            ProcessPhase::WaitingForRadar,
            Duration::from_secs(1),
        )
        .await;

        let started = std::time::Instant::now();
        drop(runtime);

        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(events.lock().unwrap().ends_with(&[
            "stop:laser".into(),
            "stop:sdr".into(),
            "stop:radar".into(),
        ]));
    }
}
