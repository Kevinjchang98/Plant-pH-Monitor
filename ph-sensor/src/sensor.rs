use std::fs::OpenOptions;
use std::io::{Read, Seek, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use platform_dirs::AppDirs;
use rand::Rng;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Reading {
    timestamp: SystemTime,
    value: f32,
}

pub struct Settings {
    pub reading_frequency: Duration,
}

#[derive(Serialize, Deserialize)]
pub struct ReadingLog {
    readings: Vec<Reading>,
}

/// Awaits for a request of a new sensor reading and returns a single Reading instance back. Will
/// exit when stop_signal is set to true
///
/// # Arguments
///
/// * `rx_reading_request`: Channel for listening for a sensor reading request on
/// * `tx_ph_value`: Channel to send an updated Reading instance through
/// * `stop_signal`: Set to true if it should stop listening for connections
pub fn sensor_loop(
    rx_reading_request: &Receiver<bool>,
    tx_ph_value: &Sender<Reading>,
    rx_settings: &Receiver<Settings>,
    stop_signal: Arc<AtomicBool>,
) {
    println!("Sensor thread started");

    // Check for new requests every second
    let tick_duration = Duration::new(0, 1_000_000_000u32);
    let mut next_automated_reading_time = SystemTime::now();

    loop {
        if stop_signal.load(Ordering::Relaxed) {
            println!("Exiting sensor thread");
            break;
        }

        match rx_settings.try_recv() {
            Ok(request) => {
                dbg!(request.reading_frequency);
            }
            _ => {}
        }

        match rx_reading_request.try_recv() {
            Ok(_) => {
                tx_ph_value.send(_get_sensor_reading()).unwrap();
            }
            _ => {
                std::thread::sleep(tick_duration);

                if next_automated_reading_time < SystemTime::now() {
                    let current_time = SystemTime::now();

                    // Set next automated reading to be next midnight
                    let duration_since_epoch = current_time
                        .duration_since(UNIX_EPOCH)
                        .expect("Failed to get duration since epoch");
                    let seconds_since_midnight = duration_since_epoch.as_secs() % (24 * 60 * 60);
                    let seconds_until_midnight = (24 * 60 * 60) - seconds_since_midnight;
                    next_automated_reading_time =
                        current_time + Duration::from_secs(seconds_until_midnight);

                    _add_reading_to_reading_log();
                }
            }
        }
    }
}

fn _add_reading_to_reading_log() {
    println!("Adding reading");
    let app_dirs = AppDirs::new(Some("ph_sensor"), false).unwrap();
    let log_path = app_dirs.data_dir.join("reading_log");

    // Create directory if it doesn't exist
    std::fs::create_dir_all(&app_dirs.data_dir).unwrap();

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(log_path)
        .expect("Unable to open log");

    let mut contents = String::new();

    file.read_to_string(&mut contents)
        .expect("Unable to read log contents");

    let mut contents = if contents.is_empty() {
        ReadingLog {
            readings: Vec::new(),
        }
    } else {
        serde_json::from_str(&contents).expect("Failed to parse old contents")
    };

    contents.readings.push(_get_sensor_reading());

    let serialized_data =
        serde_json::to_string_pretty(&contents).expect("Unable to serialize new data");

    file.seek(std::io::SeekFrom::Start(0))
        .expect("Unable to seek to beginning");
    file.set_len(0).expect("Unable to truncate file");
    file.write_all(serialized_data.as_bytes())
        .expect("Unable to write to file");

    println!("Log updated");
}

fn _get_sensor_reading() -> Reading {
    let mut rng = rand::thread_rng();

    Reading {
        timestamp: SystemTime::now(),
        value: rng.gen::<f32>() * 14.0,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{Receiver, Sender};
    use std::sync::{mpsc, Arc};
    use std::time::Duration;

    use crate::sensor::{sensor_loop, Reading, Settings};

    #[test]
    fn sensor_loop_stops_on_stop_signal() {
        let (_, rx_reading_request): (Sender<bool>, Receiver<bool>) = mpsc::channel();
        let (tx_ph_value, _): (Sender<Reading>, Receiver<Reading>) = mpsc::channel();
        let (_, rx_settings): (Sender<Settings>, Receiver<Settings>) = mpsc::channel();
        let stop_signal = Arc::new(AtomicBool::new(false));
        let stop_signal_clone = Arc::clone(&stop_signal);

        let sensor_loop_thread = std::thread::spawn(move || {
            sensor_loop(
                &rx_reading_request,
                &tx_ph_value,
                &rx_settings,
                stop_signal_clone,
            )
        });

        println!("Sending stop signal");
        stop_signal.store(true, Ordering::Relaxed);

        // We expect the thread to stop reasonably soon after the stop signal is set
        std::thread::sleep(Duration::new(5, 0));

        assert!(sensor_loop_thread.is_finished());
    }

    #[test]
    fn sensor_loop_requests_reading_on_channel_request() {
        let (tx_reading_request, rx_reading_request): (Sender<bool>, Receiver<bool>) =
            mpsc::channel();
        let (tx_ph_value, rx_ph_value): (Sender<Reading>, Receiver<Reading>) = mpsc::channel();
        let (_, rx_settings): (Sender<Settings>, Receiver<Settings>) = mpsc::channel();
        let stop_signal = Arc::new(AtomicBool::new(false));
        let stop_signal_clone = Arc::clone(&stop_signal);

        std::thread::spawn(move || {
            sensor_loop(
                &rx_reading_request,
                &tx_ph_value,
                &rx_settings,
                stop_signal_clone,
            )
        });

        println!("Sending request");
        tx_reading_request
            .send(true)
            .expect("Error sending tx_reading_request");

        println!("Expecting a result");
        rx_ph_value.recv().unwrap();

        println!("Sending stop signal");
        stop_signal.store(true, Ordering::Relaxed);
    }
}
