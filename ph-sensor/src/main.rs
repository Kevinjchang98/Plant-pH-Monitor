use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{mpsc, Arc};
use std::time::{Duration, SystemTime};
use std::{
    io,
    io::{prelude::*, BufReader},
    net::{TcpListener, TcpStream},
};

use ctrlc;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Reading {
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
fn sensor_loop(
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

/// Listens for incoming TCP connections and spawns a new thread for each to handle. Non-blocking
/// and will exit when the stop_signal is true
///
/// # Arguments
///
/// * `tx_reading_request`: Channel for sending a request for a new Reading
/// * `rx_ph_value`: Channel for Reading response
/// * `stop_signal`: Set to true if it should stop listening for connections
fn handle_connections(
    tx_reading_request: &Sender<bool>,
    rx_ph_value: &Receiver<Reading>,
    stop_signal: Arc<AtomicBool>,
) {
    println!("Server thread started");

    // Check for new incoming connections each second
    let tick_duration = Duration::new(0, 1_000_000_000u32);

    let listener = TcpListener::bind("[::]:24000").unwrap();
    listener
        .set_nonblocking(true)
        .expect("Unable to set listener as non-blocking");

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                println!("Connection established");

                handle_client(s, &tx_reading_request, &rx_ph_value);
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                // Check stop_signal and stop listening if true
                if stop_signal.load(Ordering::Relaxed) {
                    drop(listener);
                    break;
                } else {
                    std::thread::sleep(tick_duration)
                }
            }
            Err(e) => {
                panic!("Encountered unexpected server error: {}", e)
            }
        }
    }

    println!("Exiting server thread");
}

/// Handles a single TCP connection
///
/// # Arguments
///
/// * `stream`: Single TcpStream to handle
/// * `tx_reading_request`: Channel to send a sensor reading request through
/// * `rx_ph_value`: Channel to receive an updated sensor Reading from
fn handle_client(
    mut stream: TcpStream,
    tx_reading_request: &Sender<bool>,
    rx_ph_value: &Receiver<Reading>,
) {
    let buf_reader = BufReader::new(&mut stream);

    let http_request: Vec<_> = buf_reader
        .lines()
        .map(|result| result.unwrap())
        .take_while(|line| !line.is_empty())
        .collect();

    println!("Request: {:#?}", http_request);

    // Request an updated reading from pH sensor thread
    tx_reading_request.send(true).unwrap();

    // Format to JSON
    let reading = rx_ph_value.recv().unwrap();
    let reading_json = serde_json::to_string(&reading).unwrap();

    // Send response back and close connection
    let res = "HTTP/1.1 200 OK\r\n\r\n".to_owned() + &*reading_json;
    stream.write_all(res.as_bytes()).unwrap();
}

fn main() {
    // Channels for passing data between socket and pH sensor threads
    let (tx_reading_request, rx_reading_request): (Sender<bool>, Receiver<bool>) = mpsc::channel();
    let (tx_ph_value, rx_ph_value): (Sender<Reading>, Receiver<Reading>) = mpsc::channel();

    // Channel for interrupting detached threads
    let stop_signal = Arc::new(AtomicBool::new(false));

    // Spawn threads
    let stop_signal_clone = Arc::clone(&stop_signal);
    let sensor_thread = std::thread::spawn(move || {
        sensor_loop(&rx_reading_request, &tx_ph_value, stop_signal_clone)
    });
    let stop_signal_clone = Arc::clone(&stop_signal);
    let server_thread = std::thread::spawn(move || {
        handle_connections(&tx_reading_request, &rx_ph_value, stop_signal_clone)
    });

    // Handle Ctrl-C inputs
    let stop_signal_clone = Arc::clone(&stop_signal);
    ctrlc::set_handler(move || {
        println!("\nCtrl-C received, stopping threads");
        stop_signal_clone.store(true, Ordering::Relaxed);
        return;
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
