use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio::time::{timeout, Duration};
use std::sync::Arc;
use log::{info, error};
use env_logger;

const BUFFER_SIZE: usize = 16384;
const CONNECTION_RETRIES: u8 = 5;
const BASE_TIMEOUT: Duration = Duration::from_secs(2);

#[tokio::main]
async fn main() -> io::Result<()> {
    env_logger::builder().filter_level(log::LevelFilter::Info).init();
    let port = get_port();
    let address = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&address).await?;

    info!("Proxy iniciado na porta {}", port);

    loop {
        let (client_stream, _) = listener.accept().await?;
        tokio::spawn(handle_client(client_stream));
    }
}

async fn handle_client(mut client_stream: TcpStream) {
    let proxy_address = determine_proxy_address(&mut client_stream).await;

    for attempt in 1..=CONNECTION_RETRIES {
        match TcpStream::connect(&proxy_address).await {
            Ok(mut server_stream) => {
                if let Err(e) = establish_data_transfer(&mut client_stream, &mut server_stream).await {
                    error!("Erro na transferência de dados: {}", e);
                }
                return;
            }
            Err(e) => {
                error!(
                    "Erro ao conectar ao servidor proxy ({}), tentativa {}/{}: {}",
                    proxy_address, attempt, CONNECTION_RETRIES, e
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }

    error!(
        "Não foi possível conectar ao servidor proxy após {} tentativas.",
        CONNECTION_RETRIES
    );
}

async fn establish_data_transfer(client_stream: &mut TcpStream, server_stream: &mut TcpStream) -> io::Result<()> {
    let (mut client_reader, mut client_writer) = client_stream.split();
    let (mut server_reader, mut server_writer) = server_stream.split();

    let client_to_server = async {
        io::copy(&mut client_reader, &mut server_writer).await?;
        server_writer.shutdown().await
    };

    let server_to_client = async {
        io::copy(&mut server_reader, &mut client_writer).await?;
        client_writer.shutdown().await
    };

    tokio::try_join!(client_to_server, server_to_client)?;

    Ok(())
}

async fn determine_proxy_address(client_stream: &mut TcpStream) -> String {
    let mut buffer = vec![0; 512];
    match timeout(BASE_TIMEOUT, client_stream.peek(&mut buffer)).await {
        Ok(Ok(bytes_peeked)) => {
            let data = String::from_utf8_lossy(&buffer[..bytes_peeked]);
            if data.contains("SSH") {
                "0.0.0.0:22".to_string()
            } else {
                "0.0.0.0:1194".to_string()
            }
        }
        _ => "0.0.0.0:1194".to_string(),
    }
}

fn get_port() -> u16 {
    std::env::args()
        .nth(1)
        .and_then(|port| port.parse().ok())
        .unwrap_or(8080)
            }
