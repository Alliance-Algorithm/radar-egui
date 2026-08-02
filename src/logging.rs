//! 分组件落盘日志：ZMQ / Serial / 其余分别写入 logs/ 下对应文件，同时回显终端。

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use log::{LevelFilter, Log, Metadata, Record};

const ZMQ_DIR: &str = "logs/zmq.log";
const SERIAL_DIR: &str = "logs/serial.log";
const APP_DIR: &str = "logs/app.log";

struct FileLogger {
    root: PathBuf,
    level: LevelFilter,
    files: Mutex<HashMap<String, File>>,
}

impl FileLogger {
    fn write_to_file(&self, target: &str, line: &str) {
        let rel = if target.contains("radar_egui::zmq") {
            ZMQ_DIR
        } else if target.contains("radar_egui::serial") {
            SERIAL_DIR
        } else {
            APP_DIR
        };
        let path = self.root.join(rel);
        let mut files = self.files.lock().unwrap();
        if !files.contains_key(rel) {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) else {
                return;
            };
            files.insert(rel.to_owned(), file);
        }
        if let Some(file) = files.get_mut(rel) {
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
        }
    }
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "[{} {:<5} {}] {}\n",
            format_timestamp(),
            record.level().as_str(),
            record.target(),
            record.args()
        );
        self.write_to_file(record.target(), &line);
        eprint!("{}", line);
    }
    fn flush(&self) {
        if let Ok(mut files) = self.files.lock() {
            for file in files.values_mut() {
                let _ = file.flush();
            }
        }
    }
}

fn format_timestamp() -> String {
    let now = std::time::SystemTime::now();
    let (secs, millis) = match now.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => (d.as_secs() as i64, d.subsec_millis()),
        Err(_) => return "1970-01-01 00:00:00.000".to_owned(),
    };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let mut out: libc::tm = unsafe { std::mem::zeroed() };
    let ts = secs as libc::time_t;
    unsafe {
        libc::localtime_r(&ts, &mut tm);
        out.tm_year = tm.tm_year;
        out.tm_mon = tm.tm_mon;
        out.tm_mday = tm.tm_mday;
        out.tm_hour = tm.tm_hour;
        out.tm_min = tm.tm_min;
        out.tm_sec = tm.tm_sec;
    }
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        out.tm_year + 1900,
        out.tm_mon + 1,
        out.tm_mday,
        out.tm_hour,
        out.tm_min,
        out.tm_sec,
        millis
    )
}

/// 初始化分组件文件日志（logs/zmq.log、logs/serial.log、logs/app.log），并回显终端。
/// 文件不可写时不影响终端输出（write_to_file 打开失败仅跳过落盘）。
pub fn init(root: &Path) {
    let level = if std::env::var("RUST_LOG").as_deref() == Ok("debug") {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };
    let _ = fs::create_dir_all(root);
    let logger = FileLogger {
        root: root.to_owned(),
        level,
        files: Mutex::new(HashMap::new()),
    };
    if let Err(err) = log::set_boxed_logger(Box::new(logger)) {
        eprintln!("[logging] set logger failed: {err}");
        return;
    }
    log::set_max_level(level);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;

    static INIT: Once = Once::new();

    fn init_once(dir: &Path) {
        INIT.call_once(|| init(dir));
    }

    #[test]
    fn routes_by_target() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_once(root);
        log::info!(target: "radar_egui::zmq::zmq", "zmq-line");
        log::info!(target: "radar_egui::serial::parser", "serial-line");
        log::info!(target: "radar_egui::app::radar_workspace", "app-line");
        log::logger().flush();
        let zmq = std::fs::read_to_string(root.join("logs/zmq.log")).unwrap();
        let serial = std::fs::read_to_string(root.join("logs/serial.log")).unwrap();
        let app = std::fs::read_to_string(root.join("logs/app.log")).unwrap();
        assert!(zmq.contains("zmq-line") && !zmq.contains("serial-line"));
        assert!(serial.contains("serial-line") && !serial.contains("app-line"));
        assert!(app.contains("app-line") && !app.contains("zmq-line"));
    }
}
