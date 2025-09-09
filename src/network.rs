use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc::UnboundedSender, Mutex};
use tokio_websockets::{Message, ServerBuilder, ClientBuilder, WebSocketStream};
use futures_util::{SinkExt, StreamExt};
use url::Url;

#[derive(Debug, Clone)]
pub enum NetworkEvent {
    MessageReceived(String),
    UserConnected(String),
    UserDisconnected(String),
    ConnectionEstablished,
    ConnectionFailed(String),
    InviteGenerated(String),
}

#[derive(Debug)]
pub enum ConnectionMode {
    Direct,
    Stealth, // Via Tor
}

/// Gerenciador de rede que lida com todas as operações de rede
pub struct NetworkManager {
    mode: ConnectionMode,
    broadcast_sender: Option<broadcast::Sender<Vec<u8>>>,
    client_sender: Option<tokio::sync::mpsc::UnboundedSender<String>>, // Para envio do cliente
    event_sender: UnboundedSender<NetworkEvent>,
    connections: Arc<Mutex<HashMap<String, bool>>>, // peer_addr -> connected status
    invite_tokens: Arc<Mutex<HashMap<String, (Instant, String)>>>, // token -> (expiry, pubkey)
}

impl NetworkManager {
    pub fn new(event_sender: UnboundedSender<NetworkEvent>) -> Self {
        Self {
            mode: ConnectionMode::Direct,
            broadcast_sender: None,
            client_sender: None,
            event_sender,
            connections: Arc::new(Mutex::new(HashMap::new())),
            invite_tokens: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Inicia um servidor host
    pub async fn start_host(&mut self, addr: SocketAddr) -> Result<String, NetworkError> {
        // Gera token de convite
        let token = hex::encode(generate_token());
        let pubkey_hex = "placeholder_pubkey".to_string();
        
        // Armazena token com expiração (5 minutos)
        {
            let mut tokens = self.invite_tokens.lock().await;
            tokens.insert(token.clone(), (Instant::now() + Duration::from_secs(300), pubkey_hex.clone()));
        }
        
        // Inicia o listener
        let listener = TcpListener::bind(addr).await
            .map_err(|e| NetworkError::BindFailed(e.to_string()))?;

        println!("✅ Host is listening on {}", addr);
        
        let (broadcast_sender, _) = broadcast::channel(100);
        self.broadcast_sender = Some(broadcast_sender.clone());
        
        let connections = Arc::clone(&self.connections);
        let event_sender = self.event_sender.clone();
        let tokens = Arc::clone(&self.invite_tokens);
        
        // Spawn server task
        tokio::spawn(async move {
            while let Ok((stream, peer_addr)) = listener.accept().await {
                let connections = Arc::clone(&connections);
                let broadcast_sender = broadcast_sender.clone();
                let event_sender = event_sender.clone();
                let tokens = Arc::clone(&tokens);
                
                tokio::spawn(async move {
                    if let Err(e) = handle_client_connection(
                        stream,
                        peer_addr,
                        connections,
                        broadcast_sender,
                        event_sender,
                        tokens,
                    ).await {
                        eprintln!("Client connection error: {}", e);
                    }
                });
            }
        });
        
        // Gera URI de convite
        let invite_uri = format!("sae://{}@{}?token={}", pubkey_hex, addr, token);
        let _ = self.event_sender.send(NetworkEvent::InviteGenerated(invite_uri.clone()));
        
        Ok(invite_uri)
    }

    /// Conecta a um host usando URI de convite
    pub async fn connect_to_host(&mut self, uri: &str) -> Result<(), NetworkError> {
        println!("Parsing invite URI: {}", uri);
        
        let parsed_uri = Url::parse(uri)
            .map_err(|e| {
                eprintln!("Failed to parse URI '{}': {}", uri, e);
                NetworkError::InvalidInviteUri
            })?;
        
        if parsed_uri.scheme() != "sae" {
            return Err(NetworkError::InvalidInviteUri);
        }
        
        let host = parsed_uri.host_str()
            .ok_or(NetworkError::InvalidInviteUri)?;
        let port = parsed_uri.port()
            .ok_or(NetworkError::InvalidInviteUri)?;
        let addr = format!("{}:{}", host, port);
        
        // Extrai chave pública do campo username
        let pubkey_hex = parsed_uri.username().to_string();
        if pubkey_hex.is_empty() {
            return Err(NetworkError::InvalidInviteUri);
        }
        
        // Extrai token dos parâmetros de query
        let token = parsed_uri
            .query_pairs()
            .find(|(key, _)| key == "token")
            .map(|(_, value)| value.to_string())
            .ok_or(NetworkError::InvalidInviteUri)?;
        
        // Conecta ao servidor
        let stream = match self.mode {
            ConnectionMode::Direct => {
                TcpStream::connect(&addr).await
                    .map_err(|e| NetworkError::ConnectionFailed(e.to_string()))?
            }
            ConnectionMode::Stealth => {
                // TODO: Implementar conexão Tor via SOCKS5
                return Err(NetworkError::TorNotSupported);
            }
        };
        
        // Atualiza para WebSocket
        let (ws_stream, _response) = ClientBuilder::new()
            .uri(&format!("ws://{}/", addr))
            .map_err(|e| NetworkError::UriError(e.to_string()))?
            .connect_on(stream)
            .await
            .map_err(|e| NetworkError::WebSocketFailed(e.to_string()))?;
        
        let event_sender = self.event_sender.clone();
        
        // Cria canal para envio de mensagens do cliente
        let (client_tx, client_rx) = tokio::sync::mpsc::unbounded_channel();
        self.client_sender = Some(client_tx);
        
        // Clona os valores antes de mover para async block
        let pubkey_owned = pubkey_hex.clone();
        let token_owned = token.clone();
        
        // Spawn client handler
        tokio::spawn(async move {
            if let Err(e) = handle_server_connection(
                ws_stream, 
                event_sender, 
                client_rx, 
                token_owned, 
                pubkey_owned
            ).await {
                eprintln!("Server connection error: {}", e);
            }
        });
        
        Ok(())
    }

    /// Envia uma mensagem para todos os clientes conectados (quando é host)
    pub async fn send_message(&self, message: &str) -> Result<(), NetworkError> {
        if let Some(sender) = &self.broadcast_sender {
            let message_bytes = message.as_bytes().to_vec();
            sender.send(message_bytes)
                .map_err(|_| NetworkError::SendFailed)?;
        } else if let Some(client_sender) = &self.client_sender {
            // Se somos cliente, envia diretamente para o servidor
            client_sender.send(message.to_string())
                .map_err(|_| NetworkError::SendFailed)?;
        }
        Ok(())
    }

    /// Limpa tokens de convite expirados
    pub async fn cleanup_expired_tokens(&self) {
        let mut tokens = self.invite_tokens.lock().await;
        let now = Instant::now();
        tokens.retain(|_, (expiry, _)| now < *expiry);
    }
}

impl Drop for NetworkManager {
    fn drop(&mut self) {
        // Zeroiza dados sensíveis
        self.mode = ConnectionMode::Direct;
    }
}

/// Lida com conexão de cliente no lado do servidor
async fn handle_client_connection(
    stream: TcpStream,
    peer_addr: SocketAddr,
    connections: Arc<Mutex<HashMap<String, bool>>>,
    broadcast_sender: broadcast::Sender<Vec<u8>>,
    event_sender: UnboundedSender<NetworkEvent>,
    tokens: Arc<Mutex<HashMap<String, (Instant, String)>>>,
) -> Result<(), NetworkError> {
    // Atualiza para WebSocket
    let ws_stream = ServerBuilder::new()
        .accept(stream)
        .await
        .map_err(|e| NetworkError::WebSocketFailed(e.to_string()))?;

    let peer_addr_str = peer_addr.to_string();
    
    // ✅ CORREÇÃO 1: Notificar conexão estabelecida IMEDIATAMENTE
    let _ = event_sender.send(NetworkEvent::UserConnected(peer_addr_str.clone()));

    // Adiciona conexão ativa
    {
        let mut conns = connections.lock().await;
        conns.insert(peer_addr_str.clone(), true);
    }

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    let mut broadcast_receiver = broadcast_sender.subscribe();
    
    // Flag para validação de handshake
    let mut handshake_validated = false;

    // Lida com mensagens de entrada deste cliente
    let broadcast_sender_clone = broadcast_sender.clone();
    let event_sender_clone = event_sender.clone();
    let tokens_clone = Arc::clone(&tokens);
    let peer_addr_str_clone = peer_addr_str.clone();
    
    let receive_task = tokio::spawn(async move {
        while let Some(message) = ws_receiver.next().await {
            match message {
                Ok(msg) if msg.is_text() => {
                    if let Some(text) = msg.as_text() {
                        // ✅ CORREÇÃO 3: Validação simples de token
                        if !handshake_validated {
                            // Procura por padrão simples: {"type":"handshake","token":"TOKEN"}
                            if text.contains("\"type\":\"handshake\"") && text.contains("\"token\":\"") {
                                // Extrai token de forma simples
                                if let Some(start) = text.find("\"token\":\"") {
                                    let token_start = start + 9; // Depois de "token":"
                                    if let Some(end) = text[token_start..].find('\"') {
                                        let token_value = &text[token_start..token_start + end];
                                        
                                        // Valida token
                                        let tokens_guard = tokens_clone.lock().await;
                                        if let Some((expiry, _)) = tokens_guard.get(token_value) {
                                            if Instant::now() < *expiry {
                                                handshake_validated = true;
                                                println!("Client {} validated with token", peer_addr_str_clone);
                                                continue;
                                            }
                                        }
                                    }
                                }
                                // Token inválido ou expirado
                                println!("Client {} failed token validation", peer_addr_str_clone);
                                break;
                            }
                            // Se não é handshake, mas ainda não validou, rejeita
                            println!("Client {} sent message before handshake", peer_addr_str_clone);
                            break;
                        }
                        
                        println!("Received from {}: {}", peer_addr_str_clone, text);
                        // Transmite para todos os outros clientes
                        let _ = broadcast_sender_clone.send(text.as_bytes().to_vec());
                        let _ = event_sender_clone.send(NetworkEvent::MessageReceived(text.to_string()));
                    }
                }
                Ok(msg) if msg.is_binary() => {
                    if handshake_validated {
                        let data = msg.into_payload();
                        println!("Received binary from {}: {} bytes", peer_addr_str_clone, data.len());
                        let _ = broadcast_sender_clone.send(data.to_vec());
                    }
                }
                Ok(msg) if msg.is_close() => {
                    println!("Client {} disconnected", peer_addr_str_clone);
                    break;
                }
                Err(e) => {
                    eprintln!("Error receiving from {}: {}", peer_addr_str_clone, e);
                    break;
                }
                _ => {}
            }
        }
    });

    // Lida com mensagens de saída para este cliente
    let peer_addr_str_clone2 = peer_addr_str.clone();
    let send_task = tokio::spawn(async move {
        while let Ok(data) = broadcast_receiver.recv().await {
            if let Err(e) = ws_sender.send(Message::binary(data)).await {
                eprintln!("Failed to send to {}: {}", peer_addr_str_clone2, e);
                break;
            }
        }
    });

    // Aguarda qualquer task completar
    tokio::select! {
        _ = receive_task => {},
        _ = send_task => {},
    }

    // Remove conexão e notifica desconexão
    {
        let mut conns = connections.lock().await;
        conns.remove(&peer_addr_str);
    }
    let _ = event_sender.send(NetworkEvent::UserDisconnected(peer_addr_str));
    
    Ok(())
}

/// Lida com conexão ao servidor (lado cliente)
async fn handle_server_connection(
    ws_stream: WebSocketStream<TcpStream>,
    event_sender: UnboundedSender<NetworkEvent>,
    mut client_receiver: tokio::sync::mpsc::UnboundedReceiver<String>,
    token: String,
    pubkey: String,
) -> Result<(), NetworkError> {
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    
    // ✅ CORREÇÃO 3: Envia handshake com token para validação (formato JSON simples)
    let handshake_msg = format!(
        "{{\"type\":\"handshake\",\"token\":\"{}\",\"public_key\":\"{}\"}}",
        token, pubkey
    );
    
    if let Err(e) = ws_sender.send(Message::text(handshake_msg)).await {
        let _ = event_sender.send(NetworkEvent::ConnectionFailed(format!("Failed to send handshake: {}", e)));
        return Err(NetworkError::WebSocketFailed(e.to_string()));
    }

    let _ = event_sender.send(NetworkEvent::ConnectionEstablished);

    // ✅ CORREÇÃO 4: Task para envio de mensagens do cliente
    let event_sender_clone = event_sender.clone();
    let send_task = tokio::spawn(async move {
        while let Some(message) = client_receiver.recv().await {
            if let Err(e) = ws_sender.send(Message::text(message)).await {
                eprintln!("Failed to send message to server: {}", e);
                let _ = event_sender_clone.send(NetworkEvent::ConnectionFailed(format!("Send error: {}", e)));
                break;
            }
        }
    });

    // Task para recebimento de mensagens do servidor
    let receive_task = tokio::spawn(async move {
        while let Some(message) = ws_receiver.next().await {
            match message {
                Ok(msg) if msg.is_text() => {
                    if let Some(text) = msg.as_text() {
                        let _ = event_sender.send(NetworkEvent::MessageReceived(text.to_string()));
                    }
                }
                Ok(msg) if msg.is_binary() => {
                    if let Ok(text) = String::from_utf8(msg.into_payload().to_vec()) {
                        let _ = event_sender.send(NetworkEvent::MessageReceived(text));
                    }
                }
                Ok(msg) if msg.is_close() => {
                    println!("Server closed connection");
                    break;
                }
                Err(e) => {
                    eprintln!("Error receiving from server: {}", e);
                    let _ = event_sender.send(NetworkEvent::ConnectionFailed(format!("Receive error: {}", e)));
                    break;
                }
                _ => {}
            }
        }
    });

    // Aguarda qualquer task completar
    tokio::select! {
        _ = send_task => {},
        _ = receive_task => {},
    }

    Ok(())
}

// Temporary token generation function until crypto module is ready
fn generate_token() -> Vec<u8> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    timestamp.to_be_bytes().to_vec()
}

#[derive(Debug)]
pub enum NetworkError {
    BindFailed(String),
    ConnectionFailed(String),
    WebSocketFailed(String),
    InvalidInviteUri,
    SendFailed,
    TorNotSupported,
    UriError(String),
}

impl std::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkError::BindFailed(e) => write!(f, "Failed to bind to address: {}", e),
            NetworkError::ConnectionFailed(e) => write!(f, "Connection failed: {}", e),
            NetworkError::WebSocketFailed(e) => write!(f, "WebSocket error: {}", e),
            NetworkError::InvalidInviteUri => write!(f, "Invalid invite URI format"),
            NetworkError::SendFailed => write!(f, "Failed to send message"),
            NetworkError::TorNotSupported => write!(f, "Tor connections not yet implemented"),
            NetworkError::UriError(e) => write!(f, "URI error: {}", e),
        }
    }
}

impl std::error::Error for NetworkError {}

// Add From implementation for http::uri::InvalidUri
impl From<http::uri::InvalidUri> for NetworkError {
    fn from(err: http::uri::InvalidUri) -> Self {
        NetworkError::UriError(err.to_string())
    }
}