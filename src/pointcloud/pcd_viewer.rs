use super::pcd_loader::{load_pcd, LoadedPcd, PcdEncoding};
use std::any::Any;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

type ProgressCallback<'a> = dyn FnMut(u64, u64) + 'a;
type Loader = dyn Fn(&Path, &mut ProgressCallback<'_>) -> Result<LoadedPcd, String> + Send + Sync;
type Launcher = dyn Fn(LoadedPcd) -> Result<(), String> + Send + Sync;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcdLoadStats {
    pub encoding: PcdEncoding,
    pub valid_points: usize,
    pub skipped_points: u64,
    pub declared_points: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcdLoadResult {
    pub stats: PcdLoadStats,
    pub elapsed: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PcdViewerStatus {
    Idle,
    Loading {
        path: PathBuf,
        loaded_points: u64,
        total_points: u64,
    },
    Launching {
        path: PathBuf,
        result: PcdLoadResult,
    },
    Ready {
        path: PathBuf,
        result: PcdLoadResult,
    },
    Failed {
        path: PathBuf,
        message: String,
        loaded: Option<PcdLoadResult>,
    },
}

enum WorkerEvent {
    Launching {
        generation: u64,
        path: PathBuf,
        result: PcdLoadResult,
    },
    Ready {
        generation: u64,
        path: PathBuf,
        result: PcdLoadResult,
    },
    Failed {
        generation: u64,
        path: PathBuf,
        message: String,
        loaded: Option<PcdLoadResult>,
    },
}

struct ProgressUpdate {
    generation: u64,
    path: PathBuf,
    loaded_points: u64,
    total_points: u64,
}

pub struct PcdViewerRuntime {
    status: PcdViewerStatus,
    busy: bool,
    events_tx: mpsc::SyncSender<WorkerEvent>,
    events_rx: mpsc::Receiver<WorkerEvent>,
    latest_progress: Arc<Mutex<Option<ProgressUpdate>>>,
    generation: u64,
    accepts_progress: bool,
    loader: Arc<Loader>,
    launcher: Arc<Launcher>,
}

impl PcdViewerRuntime {
    pub fn new() -> Self {
        Self::with_boundaries(
            |path, progress| load_pcd(path, progress).map_err(|error| error.to_string()),
            launch_viewer,
        )
    }

    fn with_boundaries(
        loader: impl Fn(&Path, &mut ProgressCallback<'_>) -> Result<LoadedPcd, String>
            + Send
            + Sync
            + 'static,
        launcher: impl Fn(LoadedPcd) -> Result<(), String> + Send + Sync + 'static,
    ) -> Self {
        let (events_tx, events_rx) = mpsc::sync_channel(2);
        Self {
            status: PcdViewerStatus::Idle,
            busy: false,
            events_tx,
            events_rx,
            latest_progress: Arc::new(Mutex::new(None)),
            generation: 0,
            accepts_progress: false,
            loader: Arc::new(loader),
            launcher: Arc::new(launcher),
        }
    }

    pub fn start(&mut self, path: PathBuf) -> bool {
        self.poll();
        if self.busy {
            return false;
        }

        self.generation = self
            .generation
            .checked_add(1)
            .expect("PCD load generation exhausted");
        let generation = self.generation;

        self.status = PcdViewerStatus::Loading {
            path: path.clone(),
            loaded_points: 0,
            total_points: 0,
        };
        self.busy = true;
        self.accepts_progress = true;
        let events = self.events_tx.clone();
        let latest_progress = Arc::clone(&self.latest_progress);
        let loader = Arc::clone(&self.loader);
        let launcher = Arc::clone(&self.launcher);
        let worker_path = path.clone();
        let spawn_result = thread::Builder::new()
            .name("pcd-viewer-worker".to_owned())
            .spawn(move || {
                run_worker(
                    generation,
                    worker_path,
                    events,
                    latest_progress,
                    loader,
                    launcher,
                )
            });
        if let Err(error) = spawn_result {
            self.status = PcdViewerStatus::Failed {
                path,
                message: format!("failed to start PCD viewer worker: {error}"),
                loaded: None,
            };
            self.busy = false;
            self.accepts_progress = false;
            return false;
        }
        true
    }

    pub fn poll(&mut self) {
        if self.busy && self.accepts_progress {
            if let Some(progress) = self
                .latest_progress
                .try_lock()
                .ok()
                .and_then(|mut progress| progress.take())
            {
                if progress.generation == self.generation {
                    self.status = PcdViewerStatus::Loading {
                        path: progress.path,
                        loaded_points: progress.loaded_points,
                        total_points: progress.total_points,
                    };
                }
            }
        }

        // One worker emits at most Launching plus one terminal lifecycle event.
        for _ in 0..2 {
            let Ok(event) = self.events_rx.try_recv() else {
                break;
            };
            let event_generation = match &event {
                WorkerEvent::Launching { generation, .. }
                | WorkerEvent::Ready { generation, .. }
                | WorkerEvent::Failed { generation, .. } => *generation,
            };
            if event_generation != self.generation {
                continue;
            }
            self.status = match event {
                WorkerEvent::Launching {
                    generation: _,
                    path,
                    result,
                } => {
                    self.accepts_progress = false;
                    PcdViewerStatus::Launching { path, result }
                }
                WorkerEvent::Ready {
                    generation: _,
                    path,
                    result,
                } => {
                    self.busy = false;
                    self.accepts_progress = false;
                    PcdViewerStatus::Ready { path, result }
                }
                WorkerEvent::Failed {
                    generation: _,
                    path,
                    message,
                    loaded,
                } => {
                    self.busy = false;
                    self.accepts_progress = false;
                    PcdViewerStatus::Failed {
                        path,
                        message,
                        loaded,
                    }
                }
            };
        }
    }

    pub fn status(&self) -> &PcdViewerStatus {
        &self.status
    }

    pub fn is_busy(&self) -> bool {
        self.busy
    }
}

impl Default for PcdViewerRuntime {
    fn default() -> Self {
        Self::new()
    }
}

fn run_worker(
    generation: u64,
    path: PathBuf,
    events: mpsc::SyncSender<WorkerEvent>,
    latest_progress: Arc<Mutex<Option<ProgressUpdate>>>,
    loader: Arc<Loader>,
    launcher: Arc<Launcher>,
) {
    let started = Instant::now();
    let loaded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let progress_path = path.clone();
        let mut progress = move |loaded_points: u64, total_points: u64| {
            if let Ok(mut progress) = latest_progress.lock() {
                *progress = Some(ProgressUpdate {
                    generation,
                    path: progress_path.clone(),
                    loaded_points,
                    total_points,
                });
            }
        };
        loader(&path, &mut progress)
    }));
    let loaded = match loaded {
        Ok(Ok(loaded)) => loaded,
        Ok(Err(message)) => {
            let _ = events.send(WorkerEvent::Failed {
                generation,
                path,
                message,
                loaded: None,
            });
            return;
        }
        Err(payload) => {
            let _ = events.send(WorkerEvent::Failed {
                generation,
                path,
                message: format!("PCD viewer worker panicked: {}", panic_message(&payload)),
                loaded: None,
            });
            return;
        }
    };
    let stats = PcdLoadStats {
        encoding: loaded.encoding,
        valid_points: loaded.positions.len(),
        skipped_points: loaded.skipped_points,
        declared_points: loaded.declared_points,
    };
    let _ = events.send(WorkerEvent::Launching {
        generation,
        path: path.clone(),
        result: PcdLoadResult {
            stats,
            elapsed: started.elapsed(),
        },
    });

    let launched = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| launcher(loaded)));
    let result = PcdLoadResult {
        stats,
        elapsed: started.elapsed(),
    };
    match launched {
        Ok(Ok(())) => {
            let _ = events.send(WorkerEvent::Ready {
                generation,
                path,
                result,
            });
        }
        Ok(Err(message)) => {
            let _ = events.send(WorkerEvent::Failed {
                generation,
                path,
                message,
                loaded: Some(result),
            });
        }
        Err(payload) => {
            let _ = events.send(WorkerEvent::Failed {
                generation,
                path,
                message: format!("PCD viewer worker panicked: {}", panic_message(&payload)),
                loaded: Some(result),
            });
        }
    }
}

fn panic_message(payload: &Box<dyn Any + Send>) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("unknown panic")
}

#[cfg(not(feature = "rerun"))]
fn launch_viewer(_loaded: LoadedPcd) -> Result<(), String> {
    Err("native PCD viewer requires the `rerun` feature".to_owned())
}

#[cfg(feature = "rerun")]
fn launch_viewer(loaded: LoadedPcd) -> Result<(), String> {
    use rerun as rr;

    let rec = rr::RecordingStreamBuilder::new("radar-pcd-viewer")
        .spawn()
        .map_err(|error| format!("failed to launch Rerun viewer: {error}"))?;
    let colors = loaded
        .colors
        .into_iter()
        .map(|color| rr::Color::from_unmultiplied_rgba(color[0], color[1], color[2], color[3]));
    rec.log(
        "world/pointcloud",
        &rr::Points3D::new(loaded.positions)
            .with_colors(colors)
            .with_radii([0.01]),
    )
    .map_err(|error| format!("failed to log point cloud: {error}"))?;
    rec.log(
        "world/axes/x",
        &rr::Arrows3D::from_vectors([(2.0, 0.0, 0.0)])
            .with_colors([rr::Color::from_rgb(255, 60, 60)]),
    )
    .map_err(|error| format!("failed to log x axis: {error}"))?;
    rec.log(
        "world/axes/y",
        &rr::Arrows3D::from_vectors([(0.0, 2.0, 0.0)])
            .with_colors([rr::Color::from_rgb(60, 255, 60)]),
    )
    .map_err(|error| format!("failed to log y axis: {error}"))?;
    rec.log(
        "world/axes/z",
        &rr::Arrows3D::from_vectors([(0.0, 0.0, 2.0)])
            .with_colors([rr::Color::from_rgb(60, 120, 255)]),
    )
    .map_err(|error| format!("failed to log z axis: {error}"))?;

    let half = 5.0_f32;
    let grid_lines = (-5..=5).flat_map(|coordinate| {
        let coordinate = coordinate as f32;
        [
            vec![[coordinate, -half, 0.0], [coordinate, half, 0.0]],
            vec![[-half, coordinate, 0.0], [half, coordinate, 0.0]],
        ]
    });
    rec.log(
        "world/ground_grid",
        &rr::LineStrips3D::new(grid_lines)
            .with_colors([rr::Color::from_unmultiplied_rgba(100, 100, 120, 80)]),
    )
    .map_err(|error| format!("failed to log ground grid: {error}"))?;
    rec.flush_with_timeout(Duration::from_secs(30)).map_err(|error| {
        format!(
            "failed to flush Rerun stream within 30 seconds: {error}; verify the Rerun viewer is running and reachable"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{
        PcdLoadResult, PcdLoadStats, PcdViewerRuntime, PcdViewerStatus, ProgressUpdate, WorkerEvent,
    };
    use crate::pointcloud::pcd_loader::{LoadedPcd, PcdEncoding};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{mpsc, Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    fn loaded(point_count: usize) -> LoadedPcd {
        LoadedPcd {
            positions: vec![[1.0, 2.0, 3.0]; point_count],
            colors: vec![[10, 20, 30, 255]; point_count],
            skipped_points: 2,
            declared_points: point_count as u64 + 2,
            encoding: PcdEncoding::Ascii,
        }
    }

    fn load_result(point_count: usize) -> PcdLoadResult {
        PcdLoadResult {
            stats: PcdLoadStats {
                encoding: PcdEncoding::Ascii,
                valid_points: point_count,
                skipped_points: 2,
                declared_points: point_count as u64 + 2,
            },
            elapsed: Duration::ZERO,
        }
    }

    fn poll_until(
        runtime: &mut PcdViewerRuntime,
        matches: impl Fn(&PcdViewerStatus) -> bool,
    ) -> PcdViewerStatus {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            runtime.poll();
            let status = runtime.status().clone();
            if matches(&status) {
                return status;
            }
            assert!(Instant::now() < deadline, "timed out at status {status:?}");
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn progress_mutex_contention_does_not_block_poll_or_start() {
        let mut polling_runtime =
            PcdViewerRuntime::with_boundaries(|_, _| Ok(loaded(1)), |_| Ok(()));
        polling_runtime.busy = true;
        polling_runtime.accepts_progress = true;
        polling_runtime
            .events_tx
            .send(WorkerEvent::Ready {
                generation: polling_runtime.generation,
                path: PathBuf::from("finished.pcd"),
                result: load_result(1),
            })
            .unwrap();
        let polling_progress = Arc::clone(&polling_runtime.latest_progress);
        let polling_guard = polling_progress.lock().unwrap();
        let (poll_entered_tx, poll_entered_rx) = mpsc::channel();
        let (poll_done_tx, poll_done_rx) = mpsc::channel();
        let poll_thread = thread::spawn(move || {
            poll_entered_tx.send(()).unwrap();
            polling_runtime.poll();
            poll_done_tx.send(polling_runtime).unwrap();
        });

        poll_entered_rx.recv().unwrap();
        let poll_result = poll_done_rx.recv_timeout(Duration::from_millis(250));
        drop(polling_guard);
        let polling_runtime = poll_result.expect("poll blocked on the progress mutex");
        poll_thread.join().unwrap();
        assert!(matches!(
            polling_runtime.status(),
            PcdViewerStatus::Ready { path, .. } if path == &PathBuf::from("finished.pcd")
        ));

        let starting_runtime = PcdViewerRuntime::with_boundaries(|_, _| Ok(loaded(1)), |_| Ok(()));
        let starting_progress = Arc::clone(&starting_runtime.latest_progress);
        let starting_guard = starting_progress.lock().unwrap();
        let (start_entered_tx, start_entered_rx) = mpsc::channel();
        let (start_done_tx, start_done_rx) = mpsc::channel();
        let start_thread = thread::spawn(move || {
            let mut runtime = starting_runtime;
            start_entered_tx.send(()).unwrap();
            let started = runtime.start(PathBuf::from("new.pcd"));
            start_done_tx.send((runtime, started)).unwrap();
        });

        start_entered_rx.recv().unwrap();
        let start_result = start_done_rx.recv_timeout(Duration::from_millis(250));
        drop(starting_guard);
        let (mut starting_runtime, started) =
            start_result.expect("start blocked on the progress mutex");
        start_thread.join().unwrap();
        assert!(started);
        poll_until(&mut starting_runtime, |status| {
            matches!(status, PcdViewerStatus::Ready { .. })
        });
    }

    #[test]
    fn stale_previous_load_progress_never_overwrites_a_new_load() {
        let (continue_load_tx, continue_load_rx) = mpsc::channel();
        let continue_load_rx = Mutex::new(continue_load_rx);
        let mut runtime = PcdViewerRuntime::with_boundaries(
            move |_, _| {
                continue_load_rx.lock().unwrap().recv().unwrap();
                Ok(loaded(1))
            },
            |_| Ok(()),
        );
        let progress = Arc::clone(&runtime.latest_progress);
        let mut progress_guard = progress.lock().unwrap();
        *progress_guard = Some(ProgressUpdate {
            generation: runtime.generation,
            path: PathBuf::from("previous.pcd"),
            loaded_points: 90,
            total_points: 100,
        });

        assert!(runtime.start(PathBuf::from("current.pcd")));
        drop(progress_guard);
        runtime.poll();

        assert!(
            matches!(runtime.status(), PcdViewerStatus::Loading { path, .. } if path == &PathBuf::from("current.pcd")),
            "stale progress replaced the new load: {:?}",
            runtime.status()
        );
        continue_load_tx.send(()).unwrap();
        poll_until(&mut runtime, |status| {
            matches!(status, PcdViewerStatus::Ready { .. })
        });
    }

    #[test]
    fn deferred_final_progress_never_regresses_launching_to_loading() {
        let mut runtime = PcdViewerRuntime::with_boundaries(|_, _| Ok(loaded(1)), |_| Ok(()));
        runtime.busy = true;
        runtime.generation = 1;
        runtime.accepts_progress = true;
        let progress = Arc::clone(&runtime.latest_progress);
        let mut progress_guard = progress.lock().unwrap();
        *progress_guard = Some(ProgressUpdate {
            generation: runtime.generation,
            path: PathBuf::from("cloud.pcd"),
            loaded_points: 100,
            total_points: 100,
        });
        runtime
            .events_tx
            .send(WorkerEvent::Launching {
                generation: runtime.generation,
                path: PathBuf::from("cloud.pcd"),
                result: load_result(100),
            })
            .unwrap();

        runtime.poll();
        assert!(matches!(
            runtime.status(),
            PcdViewerStatus::Launching { .. }
        ));
        drop(progress_guard);
        runtime.poll();

        assert!(
            matches!(runtime.status(), PcdViewerStatus::Launching { .. }),
            "deferred progress regressed lifecycle state: {:?}",
            runtime.status()
        );
    }

    #[test]
    fn reports_progress_before_a_blocked_load_finishes_then_transitions_in_order() {
        let (continue_load_tx, continue_load_rx) = mpsc::channel();
        let (launcher_entered_tx, launcher_entered_rx) = mpsc::channel();
        let (continue_launch_tx, continue_launch_rx) = mpsc::channel();
        let continue_load_rx = Mutex::new(continue_load_rx);
        let continue_launch_rx = Mutex::new(continue_launch_rx);
        let mut runtime = PcdViewerRuntime::with_boundaries(
            move |_, progress| {
                progress(1_500_000, 3_000_000);
                continue_load_rx.lock().unwrap().recv().unwrap();
                Ok(loaded(3))
            },
            move |_| {
                launcher_entered_tx.send(()).unwrap();
                continue_launch_rx.lock().unwrap().recv().unwrap();
                Ok(())
            },
        );
        let path = PathBuf::from("large-cloud.pcd");

        assert!(runtime.start(path.clone()));
        let progress = poll_until(&mut runtime, |status| {
            matches!(
                status,
                PcdViewerStatus::Loading {
                    loaded_points: 1_500_000,
                    total_points: 3_000_000,
                    ..
                }
            )
        });
        assert!(runtime.is_busy());
        assert_eq!(
            progress,
            PcdViewerStatus::Loading {
                path: path.clone(),
                loaded_points: 1_500_000,
                total_points: 3_000_000,
            }
        );

        continue_load_tx.send(()).unwrap();
        launcher_entered_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        assert!(matches!(
            poll_until(&mut runtime, |status| matches!(status, PcdViewerStatus::Launching { .. })),
            PcdViewerStatus::Launching { path: status_path, result }
                if status_path == path
                    && result.stats == load_result(3).stats
                    && result.elapsed <= Duration::from_secs(2)
        ));

        continue_launch_tx.send(()).unwrap();
        assert!(matches!(
            poll_until(&mut runtime, |status| matches!(status, PcdViewerStatus::Ready { .. })),
            PcdViewerStatus::Ready { path: status_path, result }
                if status_path == path
                    && result.stats == load_result(3).stats
                    && result.elapsed <= Duration::from_secs(2)
        ));
        assert!(!runtime.is_busy());
    }

    #[test]
    fn launcher_owns_loaded_pcd_and_ready_reports_complete_metadata() {
        let (positions_tx, positions_rx) = mpsc::channel();
        let mut runtime = PcdViewerRuntime::with_boundaries(
            |_, _| Ok(loaded(3)),
            move |loaded| {
                positions_tx.send(loaded.positions).unwrap();
                Ok(())
            },
        );

        assert!(runtime.start(PathBuf::from("owned.pcd")));

        let status = poll_until(&mut runtime, |status| {
            matches!(status, PcdViewerStatus::Ready { .. })
        });
        assert_eq!(positions_rx.recv().unwrap(), vec![[1.0, 2.0, 3.0]; 3]);
        match status {
            PcdViewerStatus::Ready { path, result } => {
                assert_eq!(path, PathBuf::from("owned.pcd"));
                assert_eq!(result.stats.encoding, PcdEncoding::Ascii);
                assert_eq!(result.stats.valid_points, 3);
                assert_eq!(result.stats.skipped_points, 2);
                assert_eq!(result.stats.declared_points, 5);
                assert!(result.elapsed <= Duration::from_secs(2));
            }
            status => panic!("expected Ready, got {status:?}"),
        }
    }

    #[test]
    fn millions_of_progress_callbacks_use_no_queue_and_preserve_terminal_state() {
        let (progress_finished_tx, progress_finished_rx) = mpsc::channel();
        let (continue_tx, continue_rx) = mpsc::channel();
        let continue_rx = Mutex::new(continue_rx);
        let mut runtime = PcdViewerRuntime::with_boundaries(
            move |_, progress| {
                for point in 1..=2_000_000 {
                    progress(point, 2_000_000);
                }
                progress_finished_tx.send(()).unwrap();
                continue_rx.lock().unwrap().recv().unwrap();
                Ok(loaded(1))
            },
            |_| Ok(()),
        );

        assert!(runtime.start(PathBuf::from("large.pcd")));
        progress_finished_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap();

        let queued_event_count = runtime.events_rx.try_iter().count();
        assert_eq!(queued_event_count, 0, "queued progress events");

        runtime.poll();
        assert!(matches!(
            runtime.status(),
            PcdViewerStatus::Loading {
                loaded_points: 2_000_000,
                total_points: 2_000_000,
                ..
            }
        ));

        continue_tx.send(()).unwrap();
        assert!(matches!(
            poll_until(&mut runtime, |status| matches!(
                status,
                PcdViewerStatus::Ready { .. }
            )),
            PcdViewerStatus::Ready { result, .. } if result.stats.valid_points == 1
        ));
    }

    #[test]
    fn rejects_a_second_start_while_the_worker_is_active() {
        let (continue_tx, continue_rx) = mpsc::channel();
        let continue_rx = Mutex::new(continue_rx);
        let mut runtime = PcdViewerRuntime::with_boundaries(
            move |_, _| {
                continue_rx.lock().unwrap().recv().unwrap();
                Ok(loaded(1))
            },
            |_| Ok(()),
        );

        assert!(runtime.start(PathBuf::from("first.pcd")));
        assert!(!runtime.start(PathBuf::from("second.pcd")));
        assert!(
            matches!(runtime.status(), PcdViewerStatus::Loading { path, .. } if path == &PathBuf::from("first.pcd"))
        );
        continue_tx.send(()).unwrap();
        poll_until(&mut runtime, |status| {
            matches!(status, PcdViewerStatus::Ready { .. })
        });
    }

    #[test]
    fn recovers_after_a_loader_error_and_accepts_another_start() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let loader_attempts = Arc::clone(&attempts);
        let mut runtime = PcdViewerRuntime::with_boundaries(
            move |_, _| {
                if loader_attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                    Err("invalid PCD".to_owned())
                } else {
                    Ok(loaded(1))
                }
            },
            |_| Ok(()),
        );

        assert!(runtime.start(PathBuf::from("broken.pcd")));
        assert!(matches!(
            poll_until(&mut runtime, |status| matches!(status, PcdViewerStatus::Failed { .. })),
            PcdViewerStatus::Failed { path, message, loaded: None }
                if path == PathBuf::from("broken.pcd") && message == "invalid PCD"
        ));
        assert!(runtime.start(PathBuf::from("valid.pcd")));
        assert!(matches!(
            poll_until(&mut runtime, |status| matches!(status, PcdViewerStatus::Ready { .. })),
            PcdViewerStatus::Ready { path, .. } if path == PathBuf::from("valid.pcd")
        ));
    }

    #[test]
    fn contains_launcher_failure_as_failed_status() {
        let mut runtime = PcdViewerRuntime::with_boundaries(
            |_, _| Ok(loaded(4)),
            |_| Err("viewer unavailable".to_owned()),
        );

        runtime.start(PathBuf::from("cloud.pcd"));

        match poll_until(&mut runtime, |status| {
            matches!(status, PcdViewerStatus::Failed { .. })
        }) {
            PcdViewerStatus::Failed {
                path,
                message,
                loaded: Some(result),
            } => {
                assert_eq!(path, PathBuf::from("cloud.pcd"));
                assert_eq!(message, "viewer unavailable");
                assert_eq!(result.stats.encoding, PcdEncoding::Ascii);
                assert_eq!(result.stats.valid_points, 4);
                assert_eq!(result.stats.skipped_points, 2);
                assert_eq!(result.stats.declared_points, 6);
                assert!(result.elapsed <= Duration::from_secs(2));
            }
            status => panic!("expected Failed with parsed metadata, got {status:?}"),
        }
    }

    #[test]
    fn converts_a_worker_panic_to_failed_status() {
        let mut runtime = PcdViewerRuntime::with_boundaries(
            |_, _| -> Result<LoadedPcd, String> { panic!("loader exploded") },
            |_| Ok(()),
        );

        runtime.start(PathBuf::from("cloud.pcd"));

        assert!(matches!(
            poll_until(&mut runtime, |status| matches!(status, PcdViewerStatus::Failed { .. })),
            PcdViewerStatus::Failed { path, message, loaded: None }
                if path == PathBuf::from("cloud.pcd") && message.contains("loader exploded")
        ));
        assert!(!runtime.is_busy());
    }
}
