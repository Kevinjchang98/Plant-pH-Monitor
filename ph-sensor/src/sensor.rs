use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Reading {
    timestamp: SystemTime,
    value: f32,
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
    stop_signal: Arc<AtomicBool>,
) {
    println!("Sensor thread started");

    // Check for new requests every second
    let tick_duration = Duration::new(0, 1_000_000_000u32);

    loop {
        if stop_signal.load(Ordering::Relaxed) {
            println!("Exiting sensor thread");
            break;
        }

        match rx_reading_request.try_recv() {
            Ok(_) => {
                tx_ph_value.send(_get_sensor_reading()).unwrap();
            }
            _ => {
                std::thread::sleep(tick_duration);
            }
        }
    }
}

fn _get_sensor_reading() -> Reading {
    return Reading {
        timestamp: SystemTime::now(),
        value: 7.0,
    };
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{Receiver, Sender};
    use std::sync::{mpsc, Arc};
    use std::time::Duration;

    use crate::sensor::{sensor_loop, Reading};

    #[test]
    fn sensor_loop_stops_on_stop_signal() {
        let (_, rx_reading_request): (Sender<bool>, Receiver<bool>) = mpsc::channel();
        let (tx_ph_value, _): (Sender<Reading>, Receiver<Reading>) = mpsc::channel();
        let stop_signal = Arc::new(AtomicBool::new(false));
        let stop_signal_clone = Arc::clone(&stop_signal);

        let sensor_loop_thread = std::thread::spawn(move || {
            sensor_loop(&rx_reading_request, &tx_ph_value, stop_signal_clone)
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
        let stop_signal = Arc::new(AtomicBool::new(false));
        let stop_signal_clone = Arc::clone(&stop_signal);

        std::thread::spawn(move || {
            sensor_loop(&rx_reading_request, &tx_ph_value, stop_signal_clone)
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
