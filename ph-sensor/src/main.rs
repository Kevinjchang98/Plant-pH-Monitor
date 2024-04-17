use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{mpsc, Arc};

use crate::sensor::Settings;

use ctrlc;

use sensor::Reading;

mod sensor;
mod server;

fn main() {
    // Channels for passing data between socket and pH sensor threads
    let (tx_reading_request, rx_reading_request): (Sender<bool>, Receiver<bool>) = mpsc::channel();
    let (tx_ph_value, rx_ph_value): (Sender<Reading>, Receiver<Reading>) = mpsc::channel();
    let (tx_settings, rx_settings): (Sender<Settings>, Receiver<Settings>) = mpsc::channel();

    // Channel for interrupting detached threads
    let stop_signal = Arc::new(AtomicBool::new(false));

    // Spawn threads
    let stop_signal_clone = Arc::clone(&stop_signal);
    let sensor_thread = std::thread::spawn(move || {
        sensor::sensor_loop(
            &rx_reading_request,
            &tx_ph_value,
            &rx_settings,
            stop_signal_clone,
        )
    });
    let stop_signal_clone = Arc::clone(&stop_signal);
    let server_thread = std::thread::spawn(move || {
        server::handle_connections(
            &tx_reading_request,
            &rx_ph_value,
            &tx_settings,
            stop_signal_clone,
        )
    });

    // Handle Ctrl-C inputs
    let stop_signal_clone = Arc::clone(&stop_signal);
    ctrlc::set_handler(move || {
        println!("\nCtrl-C received, stopping threads");
        stop_signal_clone.store(true, Ordering::Relaxed);
    })
    .expect("Error setting Ctrl-C handler");

    // Infinite loop unless we get a stop signal
    loop {
        if stop_signal.load(Ordering::Relaxed) {
            // Wait for child threads to join
            sensor_thread.join().unwrap();
            server_thread.join().unwrap();

            println!("Exiting main thread");
            break;
        }
    }
}
