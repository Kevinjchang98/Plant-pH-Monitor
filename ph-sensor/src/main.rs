use std::sync::{Arc, mpsc};
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, SystemTime};
use std::{io::{prelude::*, BufReader}, io, net::{TcpListener, TcpStream}};
use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, Ordering};

use threadpool::ThreadPool;
use ctrlc;

struct Reading {
    timestamp: SystemTime,
    value: f32,
}

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
            Ok(reading_request) => {
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
        value: 0.0,
    };
}

fn handle_connections(stop_signal: Arc<AtomicBool>) {
    println!("Server thread started");

    // Check for new incoming connections each second
    let tick_duration = Duration::new(0, 1_000_000_000u32);

    let listener = TcpListener::bind("[::]:24000").unwrap();
    listener.set_nonblocking(true).expect("Unable to set listener as non-blocking");
    let pool = ThreadPool::new(4);

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                println!("Connection established");

                pool.execute(|| {
                    handle_client(s);
                })
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                // Check stop_signal and stop listening if true
                if stop_signal.load(Ordering::Relaxed) {
                    break;
                } else {
                    std::thread::sleep(tick_duration)
                }
            }
            Err(e) => {panic!("Encountered unexpected server error: {}", e)}
        }
    }
    println!("Exiting server thread");
}

fn handle_client(mut stream: TcpStream) {
    let buf_reader = BufReader::new(&mut stream);

    let http_request: Vec<_> = buf_reader
        .lines()
        .map(|result| result.unwrap())
        .take_while(|line| !line.is_empty())
        .collect();

    println!("Request: {:#?}", http_request);

    let res = "HTTP/1.1 200 OK\r\n\r\n";

    stream.write_all(res.as_bytes()).unwrap();
}

fn main() {
    // Channels for passing datas between socket and pH sensor threads
    let (tx_reading_request, rx_reading_request): (Sender<bool>, Receiver<bool>) = mpsc::channel();
    let (tx_ph_value, rx_ph_value): (Sender<Reading>, Receiver<Reading>) = mpsc::channel();

    // Channel for interrupting detached threads
    let (tx_stop, rx_stop): (Sender<bool>, Receiver<bool>) = mpsc::channel();
    let stop_signal = Arc::new(AtomicBool::new(false));

    // Spawn threads
    let stop_signal_clone = Arc::clone(&stop_signal);
    let sensor_thread =
        std::thread::spawn(move || sensor_loop(&rx_reading_request, &tx_ph_value, stop_signal_clone));
    let stop_signal_clone = Arc::clone(&stop_signal);
    let server_thread = std::thread::spawn(move || handle_connections(stop_signal_clone));

    // Handle Ctrl-C inputs
    let stop_signal_clone = Arc::clone(&stop_signal);
    ctrlc::set_handler(move || {
        println!("\nCtrl-C received, stopping threads");
        stop_signal_clone.store(true, Ordering::Relaxed);
        return
    }).expect("Error setting Ctrl-C handler");

    // Infinite loop unless we get a stop signal
    loop {
        if stop_signal.load(Ordering::Relaxed) {
            // Wait for child threads to join
            sensor_thread.join();
            server_thread.join();

            println!("Exiting main thread");
            break;
        }
    }
}
