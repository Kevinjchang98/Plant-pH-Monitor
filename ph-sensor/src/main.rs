use std::{
    io::{prelude::*, BufReader},
    net::{TcpListener, TcpStream},
};

use threadpool::ThreadPool;

fn main() {
    handle_connections();
}

fn handle_connections() {
    let listener = TcpListener::bind("[::]:24000").unwrap();
    let pool = ThreadPool::new(4);

    for stream in listener.incoming() {
        let stream = stream.unwrap();

        println!("Connection established");

        pool.execute(|| {
            handle_client(stream);
        })
    }
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
