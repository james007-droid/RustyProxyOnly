use std::io::{Error, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::sync::mpsc;
use std::time::Duration;
use std::{env, thread};
use threadpool::ThreadPool;

const MAX_THREADS: usize = 16; // Aumentando o número de threads para melhor paralelismo.
const BUFFER_SIZE: usize = 4096; // Buffer maior para transferências mais rápidas.

fn main() {
    let port = get_port();
    let address = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&address).expect("Erro ao iniciar o listener");

    println!("Proxy iniciado na porta {}", port);
    start_proxy(listener);
}

fn start_proxy(listener: TcpListener) {
    let pool = ThreadPool::new(MAX_THREADS);

    for stream in listener.incoming() {
        match stream {
            Ok(client_stream) => {
                let client_stream = Arc::new(Mutex::new(client_stream));
                pool.execute(move || handle_client(client_stream));
            }
            Err(e) => eprintln!("Erro ao aceitar conexão: {}", e),
        }
    }
}

fn handle_client(client_stream: Arc<Mutex<TcpStream>>) {
    configure_timeouts(&client_stream);

    let proxy_address = determine_proxy_address(&client_stream);
    match TcpStream::connect(&proxy_address) {
        Ok(server_stream) => {
            let server_read = Arc::new(Mutex::new(server_stream.try_clone().unwrap()));
            let server_write = Arc::new(Mutex::new(server_stream));

            let client_read = Arc::clone(&client_stream);
            let client_write = Arc::clone(&client_stream);

            thread::spawn(move || transfer_data(client_read, server_write));
            thread::spawn(move || transfer_data(server_read, client_write));
        }
        Err(e) => eprintln!("Erro ao conectar ao servidor proxy ({}): {}", proxy_address, e),
    }
}

fn determine_proxy_address(client_stream: &Arc<Mutex<TcpStream>>) -> String {
    let (tx, rx) = mpsc::channel();
    let client_stream_clone = Arc::clone(client_stream);

    thread::spawn(move || {
        let result = {
            if let Ok(client) = client_stream_clone.lock() {
                peek_stream(&client)
            } else {
                Err(Error::new(std::io::ErrorKind::Other, "Erro ao obter lock no stream"))
            }
        };
        tx.send(result).ok();
    });

    if let Ok(Ok(data)) = rx.recv_timeout(Duration::from_secs(1)) {
        if data.contains("SSH") {
            "0.0.0.0:22".to_string()
        } else {
            "0.0.0.0:1194".to_string()
        }
    } else {
        "0.0.0.0:1194".to_string()
    }
}

fn transfer_data(read_stream: Arc<Mutex<TcpStream>>, write_stream: Arc<Mutex<TcpStream>>) {
    let mut buffer = vec![0; BUFFER_SIZE];
    loop {
        let bytes_read = {
            let mut reader = match read_stream.lock() {
                Ok(r) => r,
                Err(_) => return,
            };
            match reader.read(&mut buffer) {
                Ok(n) if n > 0 => n,
                _ => return,
            }
        };

        if let Ok(mut writer) = write_stream.lock() {
            if writer.write_all(&buffer[..bytes_read]).is_err() {
                break;
            }
        }
    }

    if let Ok(mut writer) = write_stream.lock() {
        writer.shutdown(Shutdown::Both).ok();
    }
}

fn configure_timeouts(client_stream: &Arc<Mutex<TcpStream>>) {
    if let Ok(mut stream) = client_stream.lock() {
        let timeout = Some(Duration::from_secs(2));
        stream.set_read_timeout(timeout).ok();
        stream.set_write_timeout(timeout).ok();
    }
}

fn peek_stream(stream: &TcpStream) -> Result<String, Error> {
    let mut buffer = vec![0; 512];
    let bytes_peeked = stream.peek(&mut buffer)?;
    Ok(String::from_utf8_lossy(&buffer[..bytes_peeked]).to_string())
}

fn get_port() -> u16 {
    env::args()
        .nth(1)
        .and_then(|port| port.parse().ok())
        .unwrap_or(8080)
}
