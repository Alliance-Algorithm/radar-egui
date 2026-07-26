use super::{ConnectionStatus, RadarApp};
use crate::shared_data::SharedData;

impl RadarApp {
    pub(super) fn update_connection_status(&mut self, snapshot: Option<&SharedData>) {
        if let Some(signal) = snapshot {
            self.connection_status = ConnectionStatus::Connected;
            self.error_message = None;
            self.data_count = self.data_count.wrapping_add(1);

            let version = self.data_count;
            if version > self.last_logged_radar_version {
                self.rerun_viz.set_frame_sequence(version as i64);
                self.rerun_viz.log_all(signal);
                self.last_logged_radar_version = version;
            }
        } else {
            self.connection_status = ConnectionStatus::Disconnected;
            self.error_message = None;
        }
    }
}
