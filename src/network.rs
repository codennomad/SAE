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
    client_sender: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    event_sender: UnboundedSender<NetworkEvent>,
    connections: Arc<Mutex<HashMap<String, bool>>>,
    invite_tokens: Arc<Mutex<HashMap<String, (Instant, String)>>>,
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

    /// Inicia um servidor host - CORRIGIDO para aceitar conexões externas
    pub async fn start_host(&mut self, addr: SocketAddr) -> Result<String, NetworkError> {
        let token = hex::encode(generate_token());
        let pubkey_hex = "placeholder_pubkey".to_string();
        
        {
            let mut tokens = self.invite_tokens.lock().await;
            tokens.insert(token.clone(), (Instant::now() + Duration::from_secs(300), pubkey_hex.clone()));
        }
        
        let listener = TcpListener::bind(addr).await
            .map_err(|e| NetworkError::BindFailed(e.to_string()))?;

        println!("✅ Host listening on {} (accepting external connections)", addr);
        
        let (broadcast_sender, _) = broadcast::channel(100);
        self.broadcast_sender = Some(broadcast_sender.clone());
        
        let connections = Arc::clone(&self.connections);
        let event_sender = self.event_sender.clone();
        let tokens = Arc::clone(&self.invite_tokens);
        
        tokio::spawn(async move {
            while let Ok((stream, peer_addr)) = listener.accept().await {
                println!("New connection from: {}", peer_addr);
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
        
        // CORRIGIDO: Usa IP local real para conexões externas
        let local_ip = get_local_ip().unwrap_or_else(|| "127.0.0.1".to_string());
        let invite_uri = format!("sae://{}@{}:{}?token={}", pubkey_hex, local_ip, addr.port(), token);
        let _ = self.event_sender.send(NetworkEvent::InviteGenerated(invite_uri.clone()));
        
        Ok(invite_uri)
    }

    pub async fn connect_to_host(&mut self, uri: &str) -> Result<(), NetworkError> {
        println!("Connecting to: {}", uri);
        
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
        
        let pubkey_hex = parsed_uri.username().to_string();
        if pubkey_hex.is_empty() {
            return Err(NetworkError::InvalidInviteUri);
        }
        
        let token = parsed_uri
            .query_pairs()
            .find(|(key, _)| key == "token")
            .map(|(_, value)| value.to_string())
            .ok_or(NetworkError::InvalidInviteUri)?;
        
        println!("Connecting to {}...", addr);
        let stream = TcpStream::connect(&addr).await
            .map_err(|e| NetworkError::ConnectionFailed(format!("Failed to connect to {}: {}", addr, e)))?;
        
        println!("TCP connection established, upgrading to WebSocket...");
        let (ws_stream, _response) = ClientBuilder::new()
            .uri(&format!("ws://{}/", addr))
            .map_err(|e| NetworkError::UriError(e.to_string()))?
            .connect_on(stream)
            .await
            .map_err(|e| NetworkError::WebSocketFailed(e.to_string()))?;
        
        println!("WebSocket connection established!");
        let event_sender = self.event_sender.clone();
        
        let (client_tx, client_rx) = tokio::sync::mpsc::unbounded_channel();
        self.client_sender = Some(client_tx);
        
        let pubkey_owned = pubkey_hex.clone();
        let token_owned = token.clone();
        
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

    pub async fn send_message(&self, message: &str) -> Result<(), NetworkError> {
        if let Some(sender) = &self.broadcast_sender {
            let message_bytes = message.as_bytes().to_vec();
            sender.send(message_bytes)
                .map_err(|_| NetworkError::SendFailed)?;
        } else if let Some(client_sender) = &self.client_sender {
            client_sender.send(message.to_string())
                .map_err(|_| NetworkError::SendFailed)?;
        }
        Ok(())
    }

    pub async fn cleanup_expired_tokens(&self) {
        let mut tokens = self.invite_tokens.lock().await;
        let now = Instant::now();
        tokens.retain(|_, (expiry, _)| now < *expiry);
    }
}

// NOVA FUNÇÃO: Obtém IP local para conexões externas
fn get_local_ip() -> Option<String> {
    use std::net::{UdpSocket, Ipv4Addr};
    
    // Tenta conectar a um endereço público para descobrir nossa interface de rede local
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                let ip = addr.ip();
                if let std::net::IpAddr::V4(ipv4) = ip {
                    if !ipv4.is_loopback() && !ipv4.is_unspecified() {
                        return Some(ipv4.to_string());
                    }
                }
            }
        }
    }
    
    // Fallback: tenta descobrir via hostname
    if let Ok(hostname) = std::process::Command::new("hostname")
        .arg("-I")
        .output()
    {
        if let Ok(output) = String::from_utf8(hostname.stdout) {
            if let Some(ip) = output.trim().split_whitespace().next() {
                if let Ok(parsed_ip) = ip.parse::<Ipv4Addr>() {
                    if !parsed_ip.is_loopback() && !parsed_ip.is_unspecified() {
                        return Some(parsed_ip.to_string());
                    }
                }
            }
        }
    }
    
    None
}

async fn handle_client_connection(
    stream: TcpStream,
    peer_addr: SocketAddr,
    connections: Arc<Mutex<HashMap<String, bool>>>,
    broadcast_sender: broadcast::Sender<Vec<u8>>,
    event_sender: UnboundedSender<NetworkEvent>,
    tokens: Arc<Mutex<HashMap<String, (Instant, String)>>>,
) -> Result<(), NetworkError> {
    let ws_stream = ServerBuilder::new()
        .accept(stream)
        .await
        .map_err(|e| NetworkError::WebSocketFailed(e.to_string()))?;

    let peer_addr_str = peer_addr.to_string();
    let _ = event_sender.send(NetworkEvent::UserConnected(peer_addr_str.clone()));

    {
        let mut conns = connections.lock().await;
        conns.insert(peer_addr_str.clone(), true);
    }

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    let mut broadcast_receiver = broadcast_sender.subscribe();
    let mut handshake_validated = false;

    let broadcast_sender_clone = broadcast_sender.clone();
    let event_sender_clone = event_sender.clone();
    let tokens_clone = Arc::clone(&tokens);
    let peer_addr_str_clone = peer_addr_str.clone();
    
    let receive_task = tokio::spawn(async move {
        while let Some(message) = ws_receiver.next().await {
            match message {
                Ok(msg) if msg.is_text() => {
                    if let Some(text) = msg.as_text() {
                        if !handshake_validated {
                            if text.contains("\"type\":\"handshake\"") && text.contains("\"token\":\"") {
                                if let Some(start) = text.find("\"token\":\"") {
                                    let token_start = start + 9;
                                    if let Some(end) = text[token_start..].find('\"') {
                                        let token_value = &text[token_start..token_start + end];
                                        
                                        let tokens_guard = tokens_clone.lock().await;
                                        if let Some((expiry, _)) = tokens_guard.get(token_value) {
                                            if Instant::now() < *expiry {
                                                handshake_validated = true;
                                                println!("✅ Client {} validated", peer_addr_str_clone);
                                                continue;
                                            }
                                        }
                                    }
                                }
                                println!("❌ Client {} failed validation", peer_addr_str_clone);
                                break;
                            }
                            println!("❌ Client {} sent message before handshake", peer_addr_str_clone);
                            break;
                        }
                        
                        println!("📨 Message from {}: {}", peer_addr_str_clone, text);
                        let _ = broadcast_sender_clone.send(text.as_bytes().to_vec());
                        let _ = event_sender_clone.send(NetworkEvent::MessageReceived(text.to_string()));
                    }
                }
                Ok(msg) if msg.is_close() => {
                    println!("🔌 Client {} disconnected", peer_addr_str_clone);
                    break;
                }
                Err(e) => {
                    eprintln!("❌ Error from {}: {}", peer_addr_str_clone, e);
                    break;
                }
                _ => {}
            }
        }
    });

    let peer_addr_str_clone2 = peer_addr_str.clone();
    let send_task = tokio::spawn(async move {
        while let Ok(data) = broadcast_receiver.recv().await {
            if let Err(e) = ws_sender.send(Message::binary(data)).await {
                eprintln!("Failed to send to {}: {}", peer_addr_str_clone2, e);
                break;
            }
        }
    });

    tokio::select! {
        _ = receive_task => {},
        _ = send_task => {},
    }

    {
        let mut conns = connections.lock().await;
        conns.remove(&peer_addr_str);
    }
    let _ = event_sender.send(NetworkEvent::UserDisconnected(peer_addr_str));
    
    Ok(())
}

async fn handle_server_connection(
    ws_stream: WebSocketStream<TcpStream>,
    event_sender: UnboundedSender<NetworkEvent>,
    mut client_receiver: tokio::sync::mpsc::UnboundedReceiver<String>,
    token: String,
    pubkey: String,
) -> Result<(), NetworkError> {
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    
    let handshake_msg = format!(
        "{{\"type\":\"handshake\",\"token\":\"{}\",\"public_key\":\"{}\"}}",
        token, pubkey
    );
    
    if let Err(e) = ws_sender.send(Message::text(handshake_msg)).await {
        let _ = event_sender.send(NetworkEvent::ConnectionFailed(format!("Handshake failed: {}", e)));
        return Err(NetworkError::WebSocketFailed(e.to_string()));
    }

    let _ = event_sender.send(NetworkEvent::ConnectionEstablished);

    let event_sender_clone = event_sender.clone();
    let send_task = tokio::spawn(async move {
        while let Some(message) = client_receiver.recv().await {
            if let Err(e) = ws_sender.send(Message::text(message)).await {
                eprintln!("Failed to send: {}", e);
                let _ = event_sender_clone.send(NetworkEvent::ConnectionFailed(format!("Send error: {}", e)));
                break;
            }
        }
    });

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
                    eprintln!("Receive error: {}", e);
                    let _ = event_sender.send(NetworkEvent::ConnectionFailed(format!("Receive error: {}", e)));
                    break;
                }
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = send_task => {},
        _ = receive_task => {},
    }

    Ok(())
}

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
            NetworkError::BindFailed(e) => write!(f, "Failed to bind: {}", e),
            NetworkError::ConnectionFailed(e) => write!(f, "Connection failed: {}", e),
            NetworkError::WebSocketFailed(e) => write!(f, "WebSocket error: {}", e),
            NetworkError::InvalidInviteUri => write!(f, "Invalid invite URI"),
            NetworkError::SendFailed => write!(f, "Failed to send"),
            NetworkError::TorNotSupported => write!(f, "Tor not supported"),
            NetworkError::UriError(e) => write!(f, "URI error: {}", e),
        }
    }
}

impl std::error::Error for NetworkError {}

impl From<http::uri::InvalidUri> for NetworkError {
    fn from(err: http::uri::InvalidUri) -> Self {
        NetworkError::UriError(err.to_string())
    }
}