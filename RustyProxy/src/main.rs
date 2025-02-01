use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration, sleep};
use std::sync::Arc;
use log::{info, error, warn};
use env_logger;

const BUFFER_SIZE: usize = 16384;
const CONNECTION_RETRIES: u8 = 10;
const BASE_TIMEOUT: Duration = Duration::from_secs(2);
const DEFAULT_PORT: u16 = 8080;
const MAX_BACKOFF: Duration = Duration::from_secs(30);

#[tokio::main]
async fn main() -> io::Result<()> {
    // Inicializa o logger
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let port = get_port();
    let address = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&address).await?;
    info!("Proxy iniciado na porta {}", port);

    // Loop principal para aceitar conexões
    loop {
        match listener.accept().await {
            Ok((client_stream, _)) => {
                let client_stream = Arc::new(Mutex::new(client_stream));
                tokio::spawn(handle_client(client_stream));
            }
            Err(e) => {
                error!("Erro ao aceitar conexão: {}", e);
            }
        }
    }
}

async fn handle_client(client_stream: Arc<Mutex<TcpStream>>) {
    let proxy_address = determine_proxy_address(&mut client_stream.lock().await).await;

    let mut backoff = BASE_TIMEOUT;
    for attempt in 1..=CONNECTION_RETRIES {
        match connect_to_proxy(&proxy_address, &client_stream).await {
            Ok(mut server_stream) => {
                info!("Conexão bem-sucedida com o servidor proxy: {}", proxy_address);
                if let Err(e) = establish_data_transfer(&client_stream, &mut server_stream).await {
                    error!("Erro na transferência de dados: {}", e);
                }
                return;
            }
            Err(e) => {
                error!(
                    "Erro ao conectar ao servidor proxy ({}), tentativa {}/{}: {}",
                    proxy_address, attempt, CONNECTION_RETRIES, e
                );
                sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF); // Backoff exponencial
            }
        }
    }

    error!(
        "Não foi possível conectar ao servidor proxy após {} tentativas.",
        CONNECTION_RETRIES
    );
}

async fn connect_to_proxy(
    proxy_address: &str,
    client_stream: &Arc<Mutex<TcpStream>>,
) -> io::Result<TcpStream> {
    let timeout_duration = Duration::from_secs(5);
    match timeout(timeout_duration, TcpStream::connect(proxy_address)).await {
        Ok(Ok(stream)) => Ok(stream),
        Ok(Err(e)) => Err(e),
        Err(_) => {
            warn!("Timeout ao conectar ao servidor proxy: {}", proxy_address);
            Err(io::Error::new(io::ErrorKind::TimedOut, "Timeout ao conectar"))
        }
    }
}

async fn establish_data_transfer(
    client_stream: &Arc<Mutex<TcpStream>>,
    server_stream: &mut TcpStream,
) -> io::Result<()> {
    let (mut client_reader, mut client_writer) = {
        let locked_client = client_stream.lock().await;
        locked_client.split()
    };
    let (mut server_reader, mut server_writer) = server_stream.split();

    let client_to_server = async {
        io::copy_buf(&mut client_reader, &mut server_writer).await?;
        server_writer.shutdown().await
    };

    let server_to_client = async {
        io::copy_buf(&mut server_reader, &mut client_writer).await?;
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
                info!("Protocolo detectado: SSH");
                "0.0.0.0:22".to_string()
            } else {
                info!("Protocolo detectado: Outro (padrão)");
                "0.0.0.0:1194".to_string()
            }
        }
        _ => {
            warn!("Não foi possível determinar o protocolo. Usando padrão.");
            "0.0.0.0:1194".to_string()
        }
    }
}

fn get_port() -> u16 {
    match std::env::args().nth(1) {
        Some(port) => match port.parse::<u16>() {
            Ok(parsed_port) => {
                info!("Porta fornecida: {}", parsed_port);
                parsed_port
            }
            Err(_) => {
                warn!("Porta inválida fornecida. Usando porta padrão: {}", DEFAULT_PORT);
                DEFAULT_PORT
            }
        },
        None => {
            info!("Nenhuma porta fornecida. Usando porta padrão: {}", DEFAULT_PORT);
            DEFAULT_PORT
        }
    }
}
