use std::io::{Error, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::sync::mpsc;
use std::time::Duration;
use std::{env, thread};

fn main() {
    // Iniciando o proxy
    let listener = TcpListener::bind(format!("0.0.0.0:{}", get_port())).expect("Erro ao iniciar o listener");
    println!("Proxy iniciado na porta {}", get_port());
    start_http(listener);
}

fn start_http(listener: TcpListener) {
    for stream in listener.incoming() {
        match stream {
            Ok(client_stream) => {
                // Uso de Arc e Mutex para compartilhamento seguro entre threads
                let client_stream = Arc::new(Mutex::new(client_stream));
                thread::spawn({
                    let client_stream = Arc::clone(&client_stream);
                    move || handle_client(client_stream)
                });
            }
            Err(e) => {
                eprintln!("Erro ao aceitar conexão: {}", e);
            }
        }
    }
}

fn handle_client(client_stream: Arc<Mutex<TcpStream>>) {
    let status = get_status();
    
    // Configuração de timeout para o stream do cliente
    if let Ok(mut client) = client_stream.lock() {
        client.set_read_timeout(Some(Duration::from_secs(5))).ok();
        client.set_write_timeout(Some(Duration::from_secs(5))).ok();
    
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

    let server_connect = TcpStream::connect(&addr_proxy);
    if let Err(e) = server_connect {
        eprintln!("Erro ao conectar ao servidor proxy: {}", e);
        return;
    }

    let server_stream = server_connect.unwrap();

    let (client_read, client_write) = (Arc::clone(&client_stream), Arc::clone(&client_stream));
    let (server_read, server_write) = (Arc::new(Mutex::new(server_stream.try_clone().unwrap())), Arc::new(Mutex::new(server_stream)));

    // Transferência de dados entre cliente e servidor usando Arc e Mutex
    thread::spawn(move || {
        transfer_data(client_read, server_write);
    });

    thread::spawn(move || {
        transfer_data(server_read, client_write);
    });
}

fn transfer_data(read_stream: Arc<Mutex<TcpStream>>, write_stream: Arc<Mutex<TcpStream>>) {
    let mut buffer = [0; 2048];
    loop {
        let bytes_read = {
            let mut read = read_stream.lock().unwrap();
            read.read(&mut buffer)
        };

        match bytes_read {
            Ok(0) => break,
            Ok(n) => {
                let write_result = {
                    let mut write = write_stream.lock().unwrap();
                    write.write_all(&buffer[..n])
                };

                if write_result.is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    if let Ok(mut write) = write_stream.lock() {
        write.shutdown(Shutdown::Both).ok();
    }
}

fn peek_stream(read_stream: &TcpStream) -> Result<String, Error> {
    let mut peek_buffer = vec![0; 1024];
    let bytes_peeked = read_stream.peek(&mut peek_buffer)?;
    let data = &peek_buffer[..bytes_peeked];
    let data_str = String::from_utf8_lossy(data);
    Ok(data_str.to_string())
}

fn get_port() -> u16 {
    let args: Vec<String> = env::args().collect();
    let mut port = 80;

    for i in 1..args.len() {
        if args[i] == "--port" {
            if i + 1 < args.len() {
                port = args[i + 1].parse().unwrap_or(80);
            }
        }
    }

    port
}

fn get_status() -> String {
    let args: Vec<String> = env::args().collect();
    let mut status = String::from("@RustyManager");

    for i in 1..args.len() {
        if args[i] == "--status" {
            if i + 1 < args.len() {
                status = args[i + 1].clone();
            }
        }
    }

    status
}
