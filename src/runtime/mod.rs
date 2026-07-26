use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

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

// ── ZMQ SUB runtime ──

pub struct ZmqSubRuntime {
    stop: Arc<AtomicBool>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl ZmqSubRuntime {
    pub fn start(
        addrs: &[String],
        zmq: Arc<Mutex<ZmqData>>,
        serial: Arc<Mutex<SerialData>>,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let addrs = addrs.to_vec();
        let sub_socket = crate::zmq::zmq::zmq_init_sub(1, &addrs).expect("ZMQ SUB init failed");
        let handle = crate::zmq::zmq::start_zmq_sub(sub_socket, zmq, serial);
        Self {
            stop,
            handle: Mutex::new(Some(handle)),
        }
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

// ── ZMQ PUB runtime ──

pub struct ZmqPubRuntime {
    stop: Arc<AtomicBool>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl ZmqPubRuntime {
    pub fn start(
        bind_addr: &str,
        zmq: Arc<Mutex<ZmqData>>,
        serial: Arc<Mutex<SerialData>>,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let pub_socket = crate::zmq::zmq::zmq_init_pub(1, bind_addr).expect("ZMQ PUB init failed");
        let handle = crate::zmq::zmq::start_zmq_pub(pub_socket, zmq, serial);
        Self {
            stop,
            handle: Mutex::new(Some(handle)),
        }
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Ok(mut h) = self.handle.lock() {
            if let Some(handle) = h.take() {
                let _ = handle.join();
            }
        }
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
