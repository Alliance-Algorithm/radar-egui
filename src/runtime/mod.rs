use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tokio::sync::watch;

use crate::laser::video::{self, VideoFrameWriter};
use crate::pointcloud::reader;
use crate::serial::data_format::SerialData;
use crate::state::{LaserObservationWriter, PointCloudFrameWriter};
use crate::zmq::data_format::ZmqData;

fn spawn_runtime_task<M, F>(make_future: M)
where
    M: FnOnce() -> F + Send + 'static,
    F: Future<Output = ()> + 'static,
{
    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        rt.block_on(make_future());
    });
}

// ── ZMQ SUB runtime (std::thread, single socket for all SUB endpoints) ──

pub struct ZmqSubRuntime {
    stop: Arc<AtomicBool>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl ZmqSubRuntime {
    pub fn start(
        addrs: &[String],
        zmq: Arc<Mutex<ZmqData>>,
        serial: Arc<Mutex<SerialData>>,
        laser_writer: LaserObservationWriter,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();
        let addrs = addrs.to_vec();

        let handle = thread::spawn(move || {
            let (_, sub, _) =
                crate::zmq::zmq::zmq_init(1, "", &addrs).expect("ZMQ SUB init failed");
            sub.set_rcvtimeo(100).expect("ZMQ set rcvtimeo");
            while !stop_clone.load(Ordering::Relaxed) {
                match sub.recv_bytes(0) {
                    Ok(bytes) => {
                        if let Ok(sdr) =
                            serde_json::from_slice::<crate::zmq::data_format::ReceiveSdr>(&bytes)
                        {
                            if let Ok(mut z) = zmq.lock() {
                                z.sdr = Some(sdr.clone());
                            }
                            let mut s = serial.lock().unwrap();
                            s.sdr_enemy_robot_position_data.hero_x = sdr.position.hero_x;
                            s.sdr_enemy_robot_position_data.hero_y = sdr.position.hero_y;
                            s.sdr_enemy_robot_position_data.engineer_x = sdr.position.engineer_x;
                            s.sdr_enemy_robot_position_data.engineer_y = sdr.position.engineer_y;
                            s.sdr_enemy_robot_position_data.infantry_3_x =
                                sdr.position.infantry_3_x;
                            s.sdr_enemy_robot_position_data.infantry_3_y =
                                sdr.position.infantry_3_y;
                            s.sdr_enemy_robot_position_data.infantry_4_x =
                                sdr.position.infantry_4_x;
                            s.sdr_enemy_robot_position_data.infantry_4_y =
                                sdr.position.infantry_4_y;
                            s.sdr_enemy_robot_position_data.aerial_x = sdr.position.aerial_x;
                            s.sdr_enemy_robot_position_data.aerial_y = sdr.position.aerial_y;
                            s.sdr_enemy_robot_position_data.sentry_x = sdr.position.sentry_x;
                            s.sdr_enemy_robot_position_data.sentry_y = sdr.position.sentry_y;
                            s.zmq_produced
                                [crate::serial::data_format::IDX_SDR_ENEMY_ROBOT_POSITION] = 1;
                            continue;
                        }
                        if let Ok(laser) =
                            serde_json::from_slice::<crate::zmq::data_format::ReceiveLaser>(&bytes)
                        {
                            if let Ok(mut z) = zmq.lock() {
                                z.laser = Some(laser.clone());
                            }
                            let observation = crate::laser::protocol::LaserObservation {
                                detected: laser.detected,
                                center: laser.center,
                                brightness: laser.brightness,
                                contour: laser.contour,
                                candidates: laser
                                    .candidates
                                    .iter()
                                    .map(|c| crate::laser::protocol::ModelCandidate {
                                        score: c.score,
                                        class_id: c.class_id,
                                        bbox: c.bbox,
                                        center: c.center,
                                    })
                                    .collect(),
                                received_at: Some(std::time::Instant::now()),
                            };
                            laser_writer.publish(observation);
                            continue;
                        }
                        if let Ok(lidar) = serde_json::from_slice::<
                            crate::zmq::data_format::ReceiveLidarLocation,
                        >(&bytes)
                        {
                            if let Ok(mut z) = zmq.lock() {
                                z.lidar = Some(lidar);
                            }
                            // Lidar fusion handled by fusion layer
                            continue;
                        }
                    }
                    Err(e) => {
                        log::warn!("ZMQ SUB recv error: {}", e);
                        thread::sleep(Duration::from_secs(1));
                    }
                }
            }
        });

        Self { stop, handle: Mutex::new(Some(handle)) }
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Ok(mut h) = self.handle.lock() {
            if let Some(handle) = h.take() {
                let _ = handle.join();
            }
        }
    }

    pub fn is_started(&self) -> bool {
        !self.stop.load(Ordering::Relaxed)
    }
}

// ── Video (SHM) ──

pub struct VideoRuntime {
    shutdown_tx: watch::Sender<bool>,
    started: bool,
    writer: VideoFrameWriter,
}

impl VideoRuntime {
    pub fn new(writer: VideoFrameWriter) -> Self {
        let (shutdown_tx, _shutdown_rx) = watch::channel(false);

        Self {
            shutdown_tx,
            started: false,
            writer,
        }
    }

    pub fn ensure_started(&mut self) {
        if self.started {
            return;
        }

        self.started = true;
        let _ = self.shutdown_tx.send(true);

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        self.shutdown_tx = shutdown_tx;
        let writer = self.writer.clone();

        spawn_runtime_task(move || async move {
            video::run_video_client(writer, shutdown_rx).await;
        });
    }
}

// ── PointCloud (SHM) ──

pub struct PointCloudRuntime {
    shutdown_tx: watch::Sender<bool>,
    started: bool,
    writer: PointCloudFrameWriter,
}

impl PointCloudRuntime {
    pub fn new(writer: PointCloudFrameWriter) -> Self {
        let (shutdown_tx, _shutdown_rx) = watch::channel(false);

        Self {
            shutdown_tx,
            started: false,
            writer,
        }
    }

    pub fn is_started(&self) -> bool {
        self.started
    }

    pub fn ensure_started(&mut self) {
        if self.started {
            return;
        }

        self.started = true;
        let _ = self.shutdown_tx.send(true);

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        self.shutdown_tx = shutdown_tx;
        let writer = self.writer.clone();

        spawn_runtime_task(move || async move {
            reader::run_pointcloud_client(writer, shutdown_rx).await;
        });
    }
}
