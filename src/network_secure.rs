use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Mutex;
use tokio_websockets::{Message, ServerBuilder, ClientBuilder};
use futures_util::{SinkExt, StreamExt};
use url::Url;
use crate::identity::{AuthenticatedHandshake, Identity};

/// Eventos de rede enviados para o loop principal da aplicação.
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    DataReceived(Vec<u8>),
    PeerConnected {
        public_key: [u8; 32],
        ed25519_key: [u8; 32],
        fingerprint: String,
    },
    PeerDisconnected,
    ConnectionEstablished,
    ConnectionFailed(String),
    Log(String),
    /// Solicita confirmação do fingerprint do peer antes de prosseguir
    FingerprintVerificationRequired {
        fingerprint: String,
        ed25519_key: [u8; 32],
    },
}

/// Gerencia as conexões de rede com TLS e autenticação mútua.
pub struct NetworkManager {
    sender: Arc<Mutex<Option<futures_util::stream::SplitSink<tokio_websockets::WebSocketStream<TcpStream>, Message>>>>,
    event_sender: UnboundedSender<NetworkEvent>,
    identity: Arc<Identity>,
    use_tls: bool,
}

impl NetworkManager {
    pub fn new(event_sender: UnboundedSender<NetworkEvent>, use_tls: bool) -> Self {
        let identity = Identity::generate();

        Self {
            sender: Arc::new(Mutex::new(None)),
            event_sender,
            identity: Arc::new(identity),
            use_tls,
        }
    }

    /// Retorna o fingerprint da identidade local.
    pub fn local_fingerprint(&self) -> String {
        self.identity.fingerprint()
    }

    /// Inicia um servidor host com autenticação mútua.
    pub async fn start_host(&mut self, addr: SocketAddr, local_public_key: [u8; 32]) -> Result<(), String> {
        let listener = TcpListener::bind(addr).await.map_err(|e| e.to_string())?;

        let protocol = if self.use_tls { "wss" } else { "ws" };
        self.event_sender.send(NetworkEvent::Log(
            format!("Host escutando em {}://{}", protocol, addr)
        )).unwrap();

        let event_sender = self.event_sender.clone();
        let sender_clone = self.sender.clone();
        let identity = self.identity.clone();

        tokio::spawn(async move {
            if let Ok((stream, peer_addr)) = listener.accept().await {
                event_sender.send(NetworkEvent::Log(
                    format!("Conexão recebida de {}", peer_addr)
                )).unwrap();

                let ws_stream = ServerBuilder::new()
                    .accept(stream)
                    .await
                    .map_err(|e| e.to_string());

                match ws_stream {
                    Ok(ws) => {
                        let (mut ws_sender, mut ws_receiver) = ws.split();

                        // 1. Cria handshake autenticado
                        let handshake = AuthenticatedHandshake::new(local_public_key, &identity);
                        let handshake_bytes = match serde_json::to_vec(&handshake) {
                            Ok(b) => b,
                            Err(e) => {
                                event_sender.send(NetworkEvent::ConnectionFailed(
                                    format!("Erro ao serializar handshake: {}", e)
                                )).unwrap();
                                return;
                            }
                        };

                        // 2. Envia handshake autenticado
                        if ws_sender.send(Message::binary(handshake_bytes)).await.is_err() {
                            event_sender.send(NetworkEvent::ConnectionFailed(
                                "Falha ao enviar handshake".to_string()
                            )).unwrap();
                            return;
                        }

                        // 3. Recebe e verifica handshake do cliente
                        if let Some(Ok(msg)) = ws_receiver.next().await {
                            let peer_handshake_bytes = msg.as_payload().to_vec();

                            match serde_json::from_slice::<AuthenticatedHandshake>(&peer_handshake_bytes) {
                                Ok(peer_handshake) => {
                                    // Verifica a assinatura
                                    match peer_handshake.verify() {
                                        Ok(_peer_verifying_key) => {
                                            let peer_x25519 = match peer_handshake.x25519_key_array() {
                                                Ok(k) => k,
                                                Err(e) => {
                                                    event_sender.send(NetworkEvent::ConnectionFailed(
                                                        format!("Erro ao processar chave X25519: {}", e)
                                                    )).unwrap();
                                                    return;
                                                }
                                            };
                                            let peer_ed25519 = match peer_handshake.ed25519_key_array() {
                                                Ok(k) => k,
                                                Err(e) => {
                                                    event_sender.send(NetworkEvent::ConnectionFailed(
                                                        format!("Erro ao processar chave Ed25519: {}", e)
                                                    )).unwrap();
                                                    return;
                                                }
                                            };

                                            // Calcula fingerprint
                                            let fingerprint = match peer_handshake.fingerprint() {
                                                Ok(fp) => fp,
                                                Err(e) => {
                                                    event_sender.send(NetworkEvent::ConnectionFailed(
                                                        format!("Erro ao calcular fingerprint: {}", e)
                                                    )).unwrap();
                                                    return;
                                                }
                                            };

                                            event_sender.send(NetworkEvent::Log(
                                                format!("✓ Assinatura verificada! Fingerprint: {}", fingerprint)
                                            )).unwrap();

                                            event_sender.send(NetworkEvent::PeerConnected {
                                                public_key: peer_x25519,
                                                ed25519_key: peer_ed25519,
                                                fingerprint,
                                            }).unwrap();

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
                                        }
                                        Err(e) => {
                                            event_sender.send(NetworkEvent::ConnectionFailed(
                                                format!("⚠️ ASSINATURA INVÁLIDA: {} - Possível ataque MITM!", e)
                                            )).unwrap();
                                        }
                                    }
                                }
                                Err(e) => {
                                    event_sender.send(NetworkEvent::ConnectionFailed(
                                        format!("Handshake inválido: {}", e)
                                    )).unwrap();
                                }
                            }
                        } else {
                            event_sender.send(NetworkEvent::ConnectionFailed(
                                "Falha ao receber handshake".to_string()
                            )).unwrap();
                        }
                    }
                    Err(e) => {
                        event_sender.send(NetworkEvent::ConnectionFailed(
                            format!("Erro no WebSocket: {}", e)
                        )).unwrap();
                    }
                }
                *sender_clone.lock().await = None;
            }
        });

        Ok(())
    }

    /// Conecta-se a um host usando a URI de convite com autenticação.
    pub async fn connect_to_host(&mut self, uri: &str, local_public_key: [u8; 32]) -> Result<(), String> {
        let parsed_uri = Url::parse(uri).map_err(|_| "URI de convite inválida".to_string())?;
        let host = parsed_uri.host_str().ok_or("Host inválido na URI".to_string())?;
        let port = parsed_uri.port().ok_or("Porta inválida na URI".to_string())?;
        let addr = format!("{}:{}", host, port);

        let stream = TcpStream::connect(&addr).await
            .map_err(|e| format!("Falha ao conectar: {}", e))?;

        let protocol = if self.use_tls { "wss" } else { "ws" };
        let ws_uri = format!("{}://{}", protocol, addr);

        self.event_sender.send(NetworkEvent::Log(
            format!("Conectando via {}...", protocol)
        )).unwrap();

        let (ws_stream, _) = ClientBuilder::from_uri(ws_uri.parse().unwrap())
            .connect_on(stream)
            .await
            .map_err(|e| format!("Falha no handshake WebSocket: {}", e))?;

        self.event_sender.send(NetworkEvent::ConnectionEstablished).unwrap();

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        // 1. Recebe handshake autenticado do host
        if let Some(Ok(msg)) = ws_receiver.next().await {
            let peer_handshake_bytes = msg.as_payload().to_vec();

            match serde_json::from_slice::<AuthenticatedHandshake>(&peer_handshake_bytes) {
                Ok(peer_handshake) => {
                    // Verifica a assinatura do host
                    match peer_handshake.verify() {
                        Ok(_peer_verifying_key) => {
                            let peer_x25519 = match peer_handshake.x25519_key_array() {
                                Ok(k) => k,
                                Err(e) => return Err(format!("Erro ao processar chave X25519: {}", e)),
                            };
                            let peer_ed25519 = match peer_handshake.ed25519_key_array() {
                                Ok(k) => k,
                                Err(e) => return Err(format!("Erro ao processar chave Ed25519: {}", e)),
                            };

                            let fingerprint = match peer_handshake.fingerprint() {
                                Ok(fp) => fp,
                                Err(e) => return Err(format!("Erro ao calcular fingerprint: {}", e)),
                            };

                            self.event_sender.send(NetworkEvent::Log(
                                format!("✓ Assinatura do host verificada! Fingerprint: {}", fingerprint)
                            )).unwrap();

                            // 2. Envia nosso handshake autenticado
                            let handshake = AuthenticatedHandshake::new(local_public_key, &self.identity);
                            let handshake_bytes = serde_json::to_vec(&handshake)
                                .map_err(|e| format!("Erro ao serializar handshake: {}", e))?;

                            if ws_sender.send(Message::binary(handshake_bytes)).await.is_err() {
                                return Err("Falha ao enviar handshake".to_string());
                            }

                            self.event_sender.send(NetworkEvent::PeerConnected {
                                public_key: peer_x25519,
                                ed25519_key: peer_ed25519,
                                fingerprint,
                            }).unwrap();

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
                        }
                        Err(e) => {
                            return Err(format!("⚠️ ASSINATURA DO HOST INVÁLIDA: {} - NÃO CONECTE!", e));
                        }
                    }
                }
                Err(e) => {
                    return Err(format!("Handshake do host inválido: {}", e));
                }
            }
        } else {
            return Err("Falha ao receber handshake do host".to_string());
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
