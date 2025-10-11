use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Mutex;
use tokio_websockets::{Message, ServerBuilder, ClientBuilder};
use futures_util::{SinkExt, StreamExt};
use url::Url;

/// Eventos de rede enviados para o loop principal da aplicação.
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    // Pacote de dados criptografados recebido.
    DataReceived(Vec<u8>),
    // Conexão de um novo par. Envia a chave pública do par.
    PeerConnected { public_key: [u8; 32] },
    PeerDisconnected,
    // A conexão foi estabelecida com sucesso.
    ConnectionEstablished,
    // A conexão falhou.
    ConnectionFailed(String),
    // Log de informações para o usuário.
    Log(String),
}

/// Gerencia as conexões de rede (host ou cliente).
pub struct NetworkManager {
    // Canal para enviar dados para o par conectado.
    sender: Arc<Mutex<Option<futures_util::stream::SplitSink<tokio_websockets::WebSocketStream<TcpStream>, Message>>>>,
    // Canal para enviar eventos de rede para a aplicação.
    event_sender: UnboundedSender<NetworkEvent>,
}

impl NetworkManager {
    pub fn new(event_sender: UnboundedSender<NetworkEvent>) -> Self {
        Self {
            sender: Arc::new(Mutex::new(None)),
            event_sender,
        }
    }

    /// Inicia um servidor host, aguardando uma conexão de cliente.
    pub async fn start_host(&mut self, addr: SocketAddr, local_public_key: [u8; 32]) -> Result<(), String> {
        let listener = TcpListener::bind(addr).await.map_err(|e| e.to_string())?;
        self.event_sender.send(NetworkEvent::Log(format!("Host escutando em {}", addr))).unwrap();

        let event_sender = self.event_sender.clone();
        let sender_clone = self.sender.clone();

        tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                let ws_stream = ServerBuilder::new()
                    .accept(stream)
                    .await
                    .map_err(|e| e.to_string());
                
                match ws_stream {
                    Ok(ws) => {
                        let (mut ws_sender, mut ws_receiver) = ws.split();
                        
                        // 1. Envia nossa chave pública para o cliente.
                        if ws_sender.send(Message::binary(local_public_key.to_vec())).await.is_err() {
                            event_sender.send(NetworkEvent::ConnectionFailed("Falha ao enviar chave pública".to_string())).unwrap();
                            return;
                        }
                        
                        // 2. Recebe a chave pública do cliente.
                        if let Some(Ok(msg)) = ws_receiver.next().await {
                            let peer_key_bytes = msg.as_payload().to_vec();
                            if peer_key_bytes.len() == 32 {
                                let mut peer_public_key = [0u8; 32];
                                peer_public_key.copy_from_slice(&peer_key_bytes);
                                event_sender.send(NetworkEvent::PeerConnected { public_key: peer_public_key }).unwrap();
                                
                                *sender_clone.lock().await = Some(ws_sender);
                                
                                // Loop para receber mensagens
                                while let Some(msg) = ws_receiver.next().await {
                                    match msg {
                                        Ok(m) if m.is_binary() => {
                                            let data = m.as_payload();
                                            event_sender.send(NetworkEvent::DataReceived(data.to_vec())).unwrap();
                                        }
                                        Ok(m) if m.is_close() => {
                                            event_sender.send(NetworkEvent::PeerDisconnected).unwrap();
                                            break;
                                        }
                                        Err(_) => {
                                            event_sender.send(NetworkEvent::PeerDisconnected).unwrap();
                                            break;
                                        }
                                        _ => {}
                                    }
                                }
                            } else {
                                event_sender.send(NetworkEvent::ConnectionFailed("Chave pública inválida recebida".to_string())).unwrap();
                            }
                        } else {
                            event_sender.send(NetworkEvent::ConnectionFailed("Falha ao receber chave pública".to_string())).unwrap();
                        }
                    }
                    Err(e) => {
                        event_sender.send(NetworkEvent::ConnectionFailed(format!("Erro no WebSocket: {}", e))).unwrap();
                    }
                }
                *sender_clone.lock().await = None;
            }
        });

        Ok(())
    }

    /// Conecta-se a um host usando a URI de convite.
    pub async fn connect_to_host(&mut self, uri: &str, local_public_key: [u8; 32]) -> Result<(), String> {
        let parsed_uri = Url::parse(uri).map_err(|_| "URI de convite inválida".to_string())?;
        let host = parsed_uri.host_str().ok_or("Host inválido na URI".to_string())?;
        let port = parsed_uri.port().ok_or("Porta inválida na URI".to_string())?;
        let addr = format!("{}:{}", host, port);

        let stream = TcpStream::connect(&addr).await.map_err(|e| format!("Falha ao conectar: {}", e))?;
        let ws_uri = format!("ws://{}", addr);

        let (ws_stream, _) = ClientBuilder::from_uri(ws_uri.parse().unwrap())
            .connect_on(stream)
            .await
            .map_err(|e| format!("Falha no handshake WebSocket: {}", e))?;

        self.event_sender.send(NetworkEvent::ConnectionEstablished).unwrap();
        
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        // 1. Recebe a chave pública do host.
        if let Some(Ok(msg)) = ws_receiver.next().await {
            let peer_key_bytes = msg.as_payload().to_vec();
            if peer_key_bytes.len() == 32 {
                let mut peer_public_key = [0u8; 32];
                peer_public_key.copy_from_slice(&peer_key_bytes);

                // 2. Envia nossa chave pública para o host.
                if ws_sender.send(Message::binary(local_public_key.to_vec())).await.is_err() {
                    return Err("Falha ao enviar chave pública".to_string());
                }
                
                self.event_sender.send(NetworkEvent::PeerConnected { public_key: peer_public_key }).unwrap();
                *self.sender.lock().await = Some(ws_sender);
                let event_sender = self.event_sender.clone();
                let sender_clone = self.sender.clone();
                
                // Loop para receber mensagens
                tokio::spawn(async move {
                    while let Some(msg) = ws_receiver.next().await {
                        match msg {
                            Ok(m) if m.is_binary() => {
                                let data = m.as_payload();
                                event_sender.send(NetworkEvent::DataReceived(data.to_vec())).unwrap();
                            }
                            Ok(m) if m.is_close() => {
                                event_sender.send(NetworkEvent::PeerDisconnected).unwrap();
                                break;
                            }
                            Err(_) => {
                                event_sender.send(NetworkEvent::PeerDisconnected).unwrap();
                                break;
                            }
                             _ => {}
                        }
                    }
                     *sender_clone.lock().await = None;
                });

            } else {
                return Err("Chave pública do host inválida".to_string());
            }
        } else {
            return Err("Falha ao receber chave pública do host".to_string());
        }

        Ok(())
    }

    /// Envia uma mensagem criptografada para o par conectado.
    pub async fn send_message(&self, data: Vec<u8>) -> Result<(), &'static str> {
        if let Some(sender) = &mut *self.sender.lock().await {
            sender.send(Message::binary(data)).await
                .map_err(|_| "Falha ao enviar mensagem")?;
            Ok(())
        } else {
            Err("Não conectado")
        }
    }
}
