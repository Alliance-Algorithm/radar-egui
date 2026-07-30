use std::fmt;
use std::io;
use std::thread::JoinHandle;
use std::time::Duration;

use tokio::sync::{mpsc, watch};

use super::script_runner::{self, LaserScript, ScriptRunner, TeamSide};

const START_ALL_DELAY: Duration = Duration::from_secs(1);
const FIFO_RETRY_DELAY: Duration = Duration::from_millis(50);
const FIFO_ATTEMPTS: u8 = 100;

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
    },
    RetryFailed,
    StartRadar(TeamSide),
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
    pub error: Option<String>,
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
    fn start_radar(&mut self, side: TeamSide) -> io::Result<()>;
    fn start_sdr(&mut self, enemy: TeamSide) -> io::Result<()>;
    fn start_laser(&mut self, script: LaserScript) -> io::Result<()>;
    fn configure_laser(&mut self, enemy: &str, stream: bool, record: bool) -> io::Result<()>;
    fn stop_radar(&mut self);
    fn stop_sdr(&mut self);
    fn stop_laser(&mut self);
}

impl ProcessBackend for ScriptRunner {
    fn start_radar(&mut self, side: TeamSide) -> io::Result<()> {
        ScriptRunner::start_radar(self, side.as_str())
    }

    fn start_sdr(&mut self, enemy: TeamSide) -> io::Result<()> {
        ScriptRunner::start_sdr(self, enemy.as_str())
    }

    fn start_laser(&mut self, script: LaserScript) -> io::Result<()> {
        ScriptRunner::start(self, script)
    }

    fn configure_laser(&mut self, enemy: &str, stream: bool, record: bool) -> io::Result<()> {
        script_runner::send_fifo(enemy)?;
        script_runner::send_fifo(if stream { "stream on" } else { "stream off" })?;
        script_runner::send_fifo(if record { "record on" } else { "record off" })
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

    loop {
        let command = if let Some(at) = deadline {
            tokio::select! {
                command = command_rx.recv() => command,
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
            command_rx.recv().await
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
            } => {
                failed = None;
                laser_configuration = None;
                deadline = None;
                sequence = Some(Sequence {
                    command: StartAllOptions {
                        side,
                        stream,
                        record,
                        laser_auto,
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
            ProcessCommand::StartRadar(side) => {
                sequence = None;
                failed = None;
                laser_configuration = None;
                deadline = None;
                snapshot.phase = ProcessPhase::StartingRadar;
                snapshot.error = None;
                publish(&snapshot_tx, &snapshot);
                finish_component(
                    backend.start_radar(side),
                    ProcessComponent::Radar,
                    &mut snapshot,
                    &snapshot_tx,
                );
            }
            ProcessCommand::StartSdr(side) => {
                sequence = None;
                failed = None;
                laser_configuration = None;
                deadline = None;
                snapshot.phase = ProcessPhase::StartingSdr;
                snapshot.error = None;
                publish(&snapshot_tx, &snapshot);
                finish_component(
                    backend.start_sdr(side.enemy()),
                    ProcessComponent::Sdr,
                    &mut snapshot,
                    &snapshot_tx,
                );
            }
            ProcessCommand::StartLaser(options) => {
                sequence = None;
                failed = None;
                laser_configuration = None;
                deadline = None;
                snapshot.phase = ProcessPhase::StartingLaser;
                snapshot.error = None;
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
                    snapshot.error = Some(error.to_string());
                    publish(&snapshot_tx, &snapshot);
                }
            }
            ProcessCommand::StopRadar => {
                sequence = None;
                failed = None;
                laser_configuration = None;
                deadline = None;
                backend.stop_radar();
                snapshot.radar = ComponentSnapshot::default();
                snapshot.phase = ProcessPhase::Idle;
                publish(&snapshot_tx, &snapshot);
            }
            ProcessCommand::StopSdr => {
                sequence = None;
                failed = None;
                laser_configuration = None;
                deadline = None;
                backend.stop_sdr();
                snapshot.sdr = ComponentSnapshot::default();
                snapshot.phase = ProcessPhase::Idle;
                publish(&snapshot_tx, &snapshot);
            }
            ProcessCommand::StopLaser => {
                sequence = None;
                failed = None;
                laser_configuration = None;
                deadline = None;
                backend.stop_laser();
                snapshot.laser = ComponentSnapshot::default();
                snapshot.phase = ProcessPhase::Idle;
                publish(&snapshot_tx, &snapshot);
            }
            ProcessCommand::StopAll => {
                sequence = None;
                failed = None;
                laser_configuration = None;
                deadline = None;
                stop_all(&mut backend, &mut snapshot, &snapshot_tx);
            }
            ProcessCommand::Shutdown => {
                stop_all(&mut backend, &mut snapshot, &snapshot_tx);
                break;
            }
        }
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
            publish(snapshot_tx, snapshot);
            match backend.start_radar(current.command.side) {
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
    snapshot.phase = ProcessPhase::Stopping;
    snapshot.error = None;
    publish(snapshot_tx, snapshot);
    backend.stop_laser();
    backend.stop_sdr();
    backend.stop_radar();
    *snapshot = ProcessSnapshot::default();
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
        configure_failures: u8,
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

        fn event(&mut self, component: ProcessComponent, event: String) -> io::Result<()> {
            self.events.lock().unwrap().push(event);
            if self.fail_once == Some(component) {
                self.fail_once = None;
                Err(io::Error::other("configured failure"))
            } else {
                Ok(())
            }
        }
    }

    impl ProcessBackend for FakeBackend {
        fn start_radar(&mut self, side: TeamSide) -> io::Result<()> {
            self.event(ProcessComponent::Radar, format!("radar:{}", side.as_str()))
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
    }

    fn start_all_red() -> ProcessCommand {
        ProcessCommand::StartAll {
            side: TeamSide::Red,
            stream: true,
            record: false,
            laser_auto: false,
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
                "radar:red",
                "sdr:blue",
                "laser:Competition",
                "fifo:enemy blue,stream on,record off",
            ]
        );
    }

    #[tokio::test]
    async fn failure_stops_later_steps_and_retry_continues() {
        let delay = Duration::from_millis(1);
        let backend = FakeBackend::fail_once(ProcessComponent::Sdr);
        let (runtime, events) = test_runtime(backend, delay);
        runtime.send(start_all_red()).unwrap();
        wait_for_failed(&runtime, ProcessComponent::Sdr, Duration::from_secs(1)).await;
        assert_eq!(events.lock().unwrap().as_slice(), ["radar:red", "sdr:blue"]);

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
}
