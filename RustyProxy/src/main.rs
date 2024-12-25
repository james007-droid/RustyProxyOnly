use std::io::{Error, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::sync::mpsc;
use std::time::Duration;
use std::{env, thread};
use threadpool::ThreadPool;

const MAX_THREADS: usize = 8;

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
    let status = get_status();

    if let Ok(mut client) = client_stream.lock() {
        configure_timeouts(&mut client);

        if client.write_all(format!("HTTP/1.1 101 {}\r\n\r\n", status).as_bytes()).is_err() {
            eprintln!("Erro ao enviar resposta inicial ao cliente.");
            return;
        }

        match peek_stream(&client) {
            Ok(data) if data.contains("HTTP") => {
                let _ = client.read(&mut vec![0; 1024]);
                if data.to_lowercase().contains("websocket") {
                    let _ = client.write_all(format!("HTTP/1.1 200 {}\r\n\r\n", status).as_bytes());
                }
            }
            Err(e) => eprintln!("Erro ao inspecionar fluxo: {}", e),
            _ => (),
        }
    }

    let mut proxy_address = "0.0.0.0:22".to_string();
    let (tx, rx) = mpsc::channel();

    {
        let client_stream_clone = Arc::clone(&client_stream);
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
    }

    if let Ok(Ok(data)) = rx.recv_timeout(Duration::from_secs(1)) {
        if !data.contains("SSH") {
            proxy_address = "0.0.0.0:1194".to_string();
        }
    }

    if let Ok(server_stream) = TcpStream::connect(&proxy_address) {
        let server_read = Arc::new(Mutex::new(server_stream.try_clone().unwrap()));
        let server_write = Arc::new(Mutex::new(server_stream));

        let client_read = Arc::clone(&client_stream);
        let client_write = Arc::clone(&client_stream);

        thread::spawn(move || transfer_data(client_read, server_write));
        thread::spawn(move || transfer_data(server_read, client_write));
    } else {
        eprintln!("Erro ao conectar ao servidor proxy: {}", proxy_address);
    }
}

fn transfer_data(read_stream: Arc<Mutex<TcpStream>>, write_stream: Arc<Mutex<TcpStream>>) {
    let mut buffer = vec![0; 2048];
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

fn configure_timeouts(stream: &mut TcpStream) {
    let timeout = Some(Duration::from_secs(2));
    stream.set_read_timeout(timeout).ok();
    stream.set_write_timeout(timeout).ok();
}

fn peek_stream(stream: &TcpStream) -> Result<String, Error> {
    let mut buffer = vec![0; 512];
    let bytes_peeked = stream.peek(&mut buffer)?;
    Ok(String::from_utf8_lossy(&buffer[..bytes_peeked]).to_string())
}

fn get_port() -> u16 {
    env::args().nth(2).and_then(|port| port.parse().ok()).unwrap_or(80)
}

fn get_status() -> String {
    env::args().nth(4).unwrap_or_else(|| "@RustyManager".to_string())
}
