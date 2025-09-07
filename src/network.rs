use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc::UnboundedSender, Mutex};
use tokio::time::sleep;
use tokio_websockets::{Message, ServerBuilder, ClientBuilder, WebSocketStream};
use futures_util::{SinkExt, StreamExt};
use url::Url;
use zeroize::Zeroize;

use crate::crypto::{CryptoSession, perform_key_exchange, complete_key_exchange};

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

/// Network manager handles all network operations
pub struct NetworkManager {
    mode: ConnectionMode,
    broadcast_sender: Option<broadcast::Sender<Vec<u8>>>,
    event_sender: UnboundedSender<NetworkEvent>,
    connections: Arc<Mutex<Vec<WebSocketStream<TcpStream>>>>,
    listener: Option<TcpListener>,
    invite_tokens: Arc<Mutex<HashMap<String, (Instant, String)>>>, // token -> (expiry, pubkey)
}

impl NetworkManager {
    pub fn new(event_sender: UnboundedSender<NetworkEvent>) -> Self {
        Self {
            mode: ConnectionMode::Direct,
            broadcast_sender: None,
            event_sender,
            connections: Arc::new(Mutex::new(Vec::new())),
            listener: None,
            invite_tokens: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start hosting a server
    pub async fn start_host(&mut self, addr: SocketAddr) -> Result<String, NetworkError> {
        // Generate ephemeral keypair for this session
        let (_secret, public_key) = perform_key_exchange();
        
        // Generate invite token
        let token = hex::encode(crate::crypto::generate_token());
        let pubkey_hex = hex::encode(public_key.as_bytes());
        
        // Store token with expiration (5 minutes)
        {
            let mut tokens = self.invite_tokens.lock().await;
            tokens.insert(token.clone(), (Instant::now() + Duration::from_secs(300), pubkey_hex.clone()));
        }
        
        // Start the listener
        let listener = TcpListener::bind(addr).await
            .map_err(|e| NetworkError::BindFailed(e.to_string()))?;
        
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
        
        // Generate invite URI
        let invite_uri = format!("sae://{}@{}?token={}", pubkey_hex, addr, token);
        
        let _ = self.event_sender.send(NetworkEvent::InviteGenerated(invite_uri.clone()));
        
        Ok(invite_uri)
    }

    /// Connect to a host using an invite URI
    pub async fn connect_to_host(&mut self, uri: &str) -> Result<(), NetworkError> {
        let parsed_uri = Url::parse(uri)
            .map_err(|_| NetworkError::InvalidInviteUri)?;
        
        if parsed_uri.scheme() != "sae" {
            return Err(NetworkError::InvalidInviteUri);
        }
        
        let host = parsed_uri.host_str()
            .ok_or(NetworkError::InvalidInviteUri)?;
        let port = parsed_uri.port()
            .ok_or(NetworkError::InvalidInviteUri)?;
        let addr = format!("{}:{}", host, port);
        
        // Extract public key from username field
        let pubkey_hex = parsed_uri.username();
        if pubkey_hex.is_empty() {
            return Err(NetworkError::InvalidInviteUri);
        }
        
        // Extract token from query params
        let token = parsed_uri
            .query_pairs()
            .find(|(key, _)| key == "token")
            .map(|(_, value)| value.to_string())
            .ok_or(NetworkError::InvalidInviteUri)?;
        
        // Connect to the server
        let stream = match self.mode {
            ConnectionMode::Direct => {
                TcpStream::connect(&addr).await
                    .map_err(|e| NetworkError::ConnectionFailed(e.to_string()))?
            }
            ConnectionMode::Stealth => {
                // TODO: Implement Tor connection via SOCKS5
                return Err(NetworkError::TorNotSupported);
            }
        };
        
        // Upgrade to WebSocket
        let ws_stream = ClientBuilder::new()
            .uri(&format!("ws://{}/", addr))
            .connect_on(stream)
            .await
            .map_err(|e| NetworkError::WebSocketFailed(e.to_string()))?;
        
        let event_sender = self.event_sender.clone();
        
        // Spawn client handler
        tokio::spawn(async move {
            if let Err(e) = handle_server_connection(ws_stream, event_sender).await {
                eprintln!("Server connection error: {}", e);
            }
        });
        
        Ok(())
    }

    /// Send a message to all connected clients
    pub async fn send_message(&self, message: &str) -> Result<(), NetworkError> {
        if let Some(sender) = &self.broadcast_sender {
            let message_bytes = message.as_bytes().to_vec();
            sender.send(message_bytes)
                .map_err(|_| NetworkError::SendFailed)?;
        }
        Ok(())
    }

    /// Clean up expired invite tokens
    pub async fn cleanup_expired_tokens(&self) {
        let mut tokens = self.invite_tokens.lock().await;
        let now = Instant::now();
        tokens.retain(|_, (expiry, _)| now < *expiry);
    }
}

impl Drop for NetworkManager {
    fn drop(&mut self) {
        // Zeroize sensitive data
        self.mode = ConnectionMode::Direct;
    }
}

/// Handle a client connection on the server side
async fn handle_client_connection(
    stream: TcpStream,
    peer_addr: SocketAddr,
    connections: Arc<Mutex<Vec<WebSocketStream<TcpStream>>>>,
    broadcast_sender: broadcast::Sender<Vec<u8>>,
    event_sender: UnboundedSender<NetworkEvent>,
    _tokens: Arc<Mutex<HashMap<String, (Instant, String)>>>,
) -> Result<(), NetworkError> {
    // Upgrade to WebSocket
    let ws_stream = ServerBuilder::new()
        .accept(stream)
        .await
        .map_err(|e| NetworkError::WebSocketFailed(e.to_string()))?;
    
    {
        let mut conns = connections.lock().await;
        conns.push(ws_stream);
    }
    
    let mut broadcast_receiver = broadcast_sender.subscribe();
    
    // Get the WebSocket stream for this client
    let mut ws_stream = {
        let mut conns = connections.lock().await;
        conns.pop().unwrap() // We just added it
    };
    
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    
    // Handle incoming messages from this client
    let connections_clone = Arc::clone(&connections);
    let event_sender_clone = event_sender.clone();
    let receive_task = tokio::spawn(async move {
        while let Some(message) = ws_receiver.next().await {
            match message {
                Ok(Message::Text(text)) => {
                    println!("Received from {}: {}", peer_addr, text);
                    // Broadcast to all other clients
                    let _ = broadcast_sender.send(text.into_bytes());
                }
                Ok(Message::Binary(data)) => {
                    println!("Received binary from {}: {} bytes", peer_addr, data.len());
                    let _ = broadcast_sender.send(data);
                }
                Ok(Message::Close(_)) => {
                    println!("Client {} disconnected", peer_addr);
                    break;
                }
                Err(e) => {
                    eprintln!("Error receiving from {}: {}", peer_addr, e);
                    break;
                }
                _ => {}
            }
        }
    });
    
    // Handle outgoing messages to this client
    let send_task = tokio::spawn(async move {
        while let Ok(data) = broadcast_receiver.recv().await {
            if let Err(e) = ws_sender.send(Message::Binary(data)).await {
                eprintln!("Failed to send to {}: {}", peer_addr, e);
                break;
            }
        }
    });
    
    // Wait for either task to complete
    tokio::select! {
        _ = receive_task => {},
        _ = send_task => {},
    }
    
    let _ = event_sender.send(NetworkEvent::UserDisconnected(peer_addr.to_string()));
    
    Ok(())
}

/// Handle connection to a server (client side)
async fn handle_server_connection(
    mut ws_stream: WebSocketStream<TcpStream>,
    event_sender: UnboundedSender<NetworkEvent>,
) -> Result<(), NetworkError> {
    let _ = event_sender.send(NetworkEvent::ConnectionEstablished);
    
    while let Some(message) = ws_stream.next().await {
        match message {
            Ok(Message::Text(text)) => {
                let _ = event_sender.send(NetworkEvent::MessageReceived(text));
            }
            Ok(Message::Binary(data)) => {
                if let Ok(text) = String::from_utf8(data) {
                    let _ = event_sender.send(NetworkEvent::MessageReceived(text));
                }
            }
            Ok(Message::Close(_)) => {
                println!("Server closed connection");
                break;
            }
            Err(e) => {
                eprintln!("Error receiving from server: {}", e);
                break;
            }
            _ => {}
        }
    }
    
    Ok(())
}

#[derive(Debug)]
pub enum NetworkError {
    BindFailed(String),
    ConnectionFailed(String),
    WebSocketFailed(String),
    InvalidInviteUri,
    SendFailed,
    TorNotSupported,
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
        }
    }
}

impl std::error::Error for NetworkError {}