use std::io;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use crate::sensor::Reading;

/// Listens for incoming TCP connections and spawns a new thread for each to handle. Non-blocking
/// and will exit when the stop_signal is true
///
/// # Arguments
///
/// * `tx_reading_request`: Channel for sending a request for a new Reading
/// * `rx_ph_value`: Channel for Reading response
/// * `stop_signal`: Set to true if it should stop listening for connections
pub fn handle_connections(
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{Receiver, Sender};
    use std::sync::{mpsc, Arc};
    use std::time::Duration;

    use crate::sensor::Reading;
    use crate::server::handle_connections;

    #[test]
    fn handle_connections_stops_on_stop_signal() {
        let (tx_reading_request, _): (Sender<bool>, Receiver<bool>) = mpsc::channel();
        let (_, rx_ph_value): (Sender<Reading>, Receiver<Reading>) = mpsc::channel();
        let stop_signal = Arc::new(AtomicBool::new(false));
        let stop_signal_clone = Arc::clone(&stop_signal);

        let handle_connections_thread = std::thread::spawn(move || {
            handle_connections(&tx_reading_request, &rx_ph_value, stop_signal_clone)
        });

        println!("Sending stop signal");
        stop_signal.store(true, Ordering::Relaxed);

        // We expect the thread to stop reasonably soon after the stop signal is set
        std::thread::sleep(Duration::new(5, 0));

        assert!(handle_connections_thread.is_finished());
    }
}
