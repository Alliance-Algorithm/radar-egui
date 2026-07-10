use std::sync::{Arc, Mutex};

use crate::laser::protocol::LaserObservation;
use crate::serial::data_format::SerialData;
use crate::zmq::data_format::{
    ReceiveSdr, TransmitGameState, TransmitRadarMarkProcess, ZmqData,
};

#[derive(Clone)]
pub struct ZmqReader {
    inner: Arc<Mutex<ZmqData>>,
}

#[derive(Clone)]
pub struct ZmqWriter {
    inner: Arc<Mutex<ZmqData>>,
}

impl Default for ZmqReader {
    fn default() -> Self {
        Self::new_pair().0
    }
}

impl ZmqReader {
    pub fn new_pair() -> (Self, ZmqWriter) {
        let inner = Arc::new(Mutex::new(ZmqData::default()));
        (
            Self { inner: inner.clone() },
            ZmqWriter { inner },
        )
    }

    pub fn snapshot(&self) -> Option<ReceiveSdr> {
        self.inner.lock().ok().and_then(|s| s.sdr.clone())
    }

    pub fn inner(&self) -> Arc<Mutex<ZmqData>> {
        self.inner.clone()
    }
}

impl ZmqWriter {
    pub fn publish_sdr(&self, signal: ReceiveSdr) {
        if let Ok(mut state) = self.inner.lock() {
            state.sdr = Some(signal);
        }
    }

    pub fn publish_game_state(&self, data: TransmitGameState) {
        if let Ok(mut state) = self.inner.lock() {
            state.game_state = Some(data);
        }
    }

    pub fn publish_radar_mark(&self, data: TransmitRadarMarkProcess) {
        if let Ok(mut state) = self.inner.lock() {
            state.radar_mark = Some(data);
        }
    }
}

// ── Serial shared state ──

#[derive(Clone)]
pub struct SerialReader {
    inner: Arc<Mutex<SerialData>>,
}

#[derive(Clone)]
pub struct SerialWriter {
    inner: Arc<Mutex<SerialData>>,
}

impl Default for SerialReader {
    fn default() -> Self {
        Self::new_pair().0
    }
}

impl SerialReader {
    pub fn new_pair() -> (Self, SerialWriter) {
        let inner = Arc::new(Mutex::new(SerialData::default()));
        (Self { inner: inner.clone() }, SerialWriter { inner })
    }

    pub fn inner(&self) -> Arc<Mutex<SerialData>> {
        self.inner.clone()
    }
}

impl SerialWriter {
    pub fn inner(&self) -> Arc<Mutex<SerialData>> {
        self.inner.clone()
    }
}

#[derive(Clone)]
pub struct LaserObservationReader {
    inner: Arc<Mutex<LaserObservation>>,
}

#[derive(Clone)]
pub struct LaserObservationWriter {
    inner: Arc<Mutex<LaserObservation>>,
}

impl Default for LaserObservationReader {
    fn default() -> Self {
        Self::new_pair().0
    }
}

impl LaserObservationReader {
    pub fn new_pair() -> (Self, LaserObservationWriter) {
        let inner = Arc::new(Mutex::new(LaserObservation::default()));

        (
            Self {
                inner: inner.clone(),
            },
            LaserObservationWriter { inner },
        )
    }

    pub fn snapshot(&self) -> Option<LaserSnapshot> {
        self.inner.lock().ok().map(|state| {
            let observation = state.clone();
            let online = observation.is_online();

            LaserSnapshot {
                observation,
                online,
            }
        })
    }
}

impl LaserObservationWriter {
    pub fn publish(&self, observation: LaserObservation) {
        if let Ok(mut state) = self.inner.lock() {
            *state = observation;
        }
    }
}

pub struct LaserSnapshot {
    pub observation: LaserObservation,
    pub online: bool,
}

use crate::pointcloud::protocol::PointCloudFrame;

#[derive(Clone)]
pub struct PointCloudFrameReader {
    inner: Arc<Mutex<Option<PointCloudFrame>>>,
}

#[derive(Clone)]
pub struct PointCloudFrameWriter {
    inner: Arc<Mutex<Option<PointCloudFrame>>>,
}

impl Default for PointCloudFrameReader {
    fn default() -> Self {
        Self::new_pair().0
    }
}

impl PointCloudFrameReader {
    pub fn new_pair() -> (Self, PointCloudFrameWriter) {
        let inner = Arc::new(Mutex::new(None));
        (
            Self {
                inner: inner.clone(),
            },
            PointCloudFrameWriter { inner },
        )
    }

    pub fn with_frame<R>(&self, read: impl FnOnce(Option<&PointCloudFrame>) -> R) -> Option<R> {
        let frame = self.inner.lock().ok()?;
        let snapshot = frame.clone();
        drop(frame);
        Some(read(snapshot.as_ref()))
    }
}

impl PointCloudFrameWriter {
    pub fn update(&self, frame: PointCloudFrame) {
        if let Ok(mut state) = self.inner.lock() {
            *state = Some(frame);
        }
    }

    pub fn clear(&self) {
        if let Ok(mut state) = self.inner.lock() {
            *state = None;
        }
    }
}
