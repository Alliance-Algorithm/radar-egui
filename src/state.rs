use std::sync::{Arc, Mutex};

use crate::laser::protocol::LaserObservation;
use crate::shared_data::SharedData;

#[derive(Clone)]
pub struct SharedReader {
    inner: Arc<Mutex<SharedData>>,
}

#[derive(Clone)]
pub struct SharedWriter {
    inner: Arc<Mutex<SharedData>>,
}

impl Default for SharedReader {
    fn default() -> Self {
        Self::new_pair().0
    }
}

impl SharedReader {
    pub fn new_pair() -> (Self, SharedWriter) {
        let inner = Arc::new(Mutex::new(SharedData::default()));
        (Self { inner: inner.clone() }, SharedWriter { inner })
    }

    pub fn snapshot(&self) -> SharedData {
        self.inner.lock().unwrap().clone()
    }

    pub fn inner(&self) -> Arc<Mutex<SharedData>> {
        self.inner.clone()
    }
}

impl SharedWriter {
    pub fn inner(&self) -> Arc<Mutex<SharedData>> {
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
        (Self { inner: inner.clone() }, LaserObservationWriter { inner })
    }

    pub fn snapshot(&self) -> Option<LaserSnapshot> {
        self.inner.lock().ok().map(|state| {
            let observation = state.clone();
            let online = observation.is_online();
            LaserSnapshot { observation, online }
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

#[derive(Clone)]
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
        (Self { inner: inner.clone() }, PointCloudFrameWriter { inner })
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
