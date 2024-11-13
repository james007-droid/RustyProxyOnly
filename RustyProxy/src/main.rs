use std::io::{Error, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::sync::mpsc;
use std::time::Duration;
use std::{env, thread};
use threadpool::ThreadPool;

const MAX_THREADS: usize = 8;

fn main() {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", get_port())).expect("Erro ao iniciar o listener");
    println!("Proxy iniciado na porta {}", get_port());
    start_http(listener);
}

fn start_http(listener: TcpListener) {
    let pool = ThreadPool::new(MAX_THREADS);

    for stream in listener.incoming() {
        match stream {
            Ok(client_stream) => {
                let client_stream = Arc::new(Mutex::new(client_stream));
                let client_stream_clone = Arc::clone(&client_stream);
                
                pool.execute(move || handle_client(client_stream_clone));
            }
            Err(e) => {
                eprintln!("Erro ao aceitar conexão: {}", e);
            }
        }
    }
}

fn handle_client(client_stream: Arc<Mutex<TcpStream>>) {
    let status = get_status();

    if let Ok(mut client) = client_stream.lock() {
        client.set_read_timeout(Some(Duration::from_secs(2))).ok();
        client.set_write_timeout(Some(Duration::from_secs(2))).ok();

        if client.write_all(format!("HTTP/1.1 101 {}\r\n\r\n", status).as_bytes()).is_err() {
            return;
        }

        match peek_stream(&client) {
            Ok(data_str) => {
                if data_str.contains("HTTP") {
                    let _ = client.read(&mut vec![0; 1024]);
                    let payload_str = data_str.to_lowercase();
                    if payload_str.contains("websocket") || payload_str.contains("ws") {
                        if client.write_all(format!("HTTP/1.1 200 {}\r\n\r\n", status).as_bytes()).is_err() {
                            return;
                        }
                    }
                }
            }
            Err(..) => return,
        }
    }

    let mut addr_proxy = "0.0.0.0:22";

    let (tx, rx) = mpsc::channel();
    let clone_client = Arc::clone(&client_stream);

    let read_handle = thread::spawn(move || {
        let result = if let Ok(client) = clone_client.lock() {
            peek_stream(&client)
        } else {
            Err(Error::new(std::io::ErrorKind::Other, "Erro ao obter lock no stream"))
        };
        tx.send(result).ok();
    });

    if let Ok(Ok(data_str)) = rx.recv_timeout(Duration::from_secs(1)) {
        if !data_str.contains("SSH") {
            addr_proxy = "0.0.0.0:1194";
        }
    }

    if let Ok(server_stream) = TcpStream::connect(&addr_proxy) {
        let (client_read, client_write) = (Arc::clone(&client_stream), Arc::clone(&client_stream));
        let (server_read, server_write) = (Arc::new(Mutex::new(server_stream.try_clone().unwrap())), Arc::new(Mutex::new(server_stream)));

        thread::spawn(move || {
            transfer_data(client_read, server_write);
        });

        thread::spawn(move || {
            transfer_data(server_read, client_write);
        });
    } else {
        eprintln!("Erro ao conectar ao servidor proxy: Não foi possível estabelecer uma conexão com o proxy");
    }
}

fn transfer_data(read_stream: Arc<Mutex<TcpStream>>, write_stream: Arc<Mutex<TcpStream>>) {
    let mut buffer = vec![0; 2048];
    loop {
        let bytes_read = {
            let mut read = match read_stream.lock() {
                Ok(r) => r,
                Err(_) => return,
            };
            match read.read(&mut buffer) {
                Ok(n) => n,
                Err(_) => return,
            }
        };

        if bytes_read == 0 {
            break;
        }

        if let Ok(mut write) = write_stream.lock() {
            if write.write_all(&buffer[..bytes_read]).is_err() {
                break;
            }
        }
    }

    if let Ok(mut write) = write_stream.lock() {
        write.shutdown(Shutdown::Both).ok();
    }
}

fn peek_stream(read_stream: &TcpStream) -> Result<String, Error> {
    let mut peek_buffer = vec![0; 512]; // reduzindo o tamanho do buffer para melhorar a velocidade de resposta
    let bytes_peeked = read_stream.peek(&mut peek_buffer)?;
    let data = &peek_buffer[..bytes_peeked];
    Ok(String::from_utf8_lossy(data).to_string())
}

fn get_port() -> u16 {
    let args: Vec<String> = env::args().collect();
    let mut port = 80;

    for i in 1..args.len() {
        if args[i] == "--port" && i + 1 < args.len() {
            port = args[i + 1].parse().unwrap_or(80);
        }
    }

    port
}

fn get_status() -> String {
    let args: Vec<String> = env::args().collect();
    let mut status = String::from("@RustyManager");

    for i in 1..args.len() {
        if args[i] == "--status" && i + 1 < args.len() {
            status = args[i + 1].clone();
        }
    }

    status
}
