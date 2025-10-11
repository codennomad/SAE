use color_eyre::eyre::Result;
use tokio::time::Duration;
use tokio::sync::mpsc;
use std::net::SocketAddr;
use clap::Parser;

mod app;
mod crypton;
mod event;
mod identity;
mod network;
mod network_secure;
mod padding;
mod ratchet;
mod tor;
mod tui;
mod ui;

use app::{App, AppMode, Action, ChatMessage};
use crypton::generate_keypair;
use event::{Event, EventHandler};
use network_secure::{NetworkManager, NetworkEvent};
use padding::{add_padding, remove_padding};
use ratchet::RatchetSession;
use ui::TuiManager;
use x25519_dalek::{PublicKey, EphemeralSecret};

/// SAE - Secure Anonymous Echo: Mensageiro criptografado e efêmero
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Ativa TLS/WSS para conexões seguras
    #[arg(long, default_value_t = false)]
    tls: bool,

    /// Ativa anonimato via Tor (requer Tor rodando em 127.0.0.1:9050)
    #[arg(long, default_value_t = false)]
    tor: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let args = Args::parse();

    // Verifica disponibilidade do Tor se solicitado
    if args.tor {
        let tor_config = tor::TorConfig::default();
        let tor_status = tor::get_tor_status(&tor_config).await;

        if !tor_status.is_available() {
            if let Some(msg) = tor_status.message() {
                eprintln!("⚠️  Tor não está disponível:\n{}", msg);
                eprintln!("\nIniciando sem Tor...\n");
            }
        } else {
            eprintln!("✓ Tor SOCKS5 disponível em {}", tor_config.proxy_addr());
        }
    }

    let mut app = App::new();
    let mut tui = TuiManager::new()?;
    tui.init()?;

    let mut events = EventHandler::new(Duration::from_millis(100));
    let (network_sender, mut network_receiver) = mpsc::unbounded_channel::<NetworkEvent>();
    let mut network = NetworkManager::new(network_sender, args.tls);

    // Exibe fingerprint local da identidade
    let local_id_fingerprint = network.local_fingerprint();
    app.add_message(
        format!("🔐 Identidade Ed25519: {}", local_id_fingerprint),
        "Sistema".into()
    );

    if args.tls {
        app.add_message("🔒 TLS/WSS: ATIVADO".to_string(), "Sistema".into());
    }
    if args.tor {
        app.add_message("🧅 Modo Tor: ATIVADO".to_string(), "Sistema".into());
    }

    let event_sender_clone = events.sender();
    tokio::spawn(async move {
        while let Some(net_event) = network_receiver.recv().await {
            let _ = event_sender_clone.send(Event::Network(net_event));
        }
    });

    let mut ratchet_session: Option<RatchetSession> = None;
    let mut secret_key: Option<EphemeralSecret> = None;

    while !app.should_quit {
        tui.draw(&mut app)?;

        match events.next().await? {
            Event::Key(key) if key.code == crossterm::event::KeyCode::Enter => {
                if let Some(action) = app.handle_input()? {
                    match action {
                        Action::GenerateInvite => {
                            let (secret, public) = generate_keypair();
                            let pubkey_bytes = public.to_bytes();
                            let fingerprint = crypton::get_fingerprint(&public);
                            app.local_fingerprint = Some(fingerprint.clone());
                            app.add_message(
                                format!("🔑 Fingerprint X25519: {}", fingerprint),
                                "Sistema".into()
                            );

                            secret_key = Some(secret);
                            let addr: SocketAddr = "0.0.0.0:9001".parse().unwrap();
                            let invite_uri = format!("sae://{}:{}?pubkey={}", "127.0.0.1", addr.port(), hex::encode(pubkey_bytes));
                            app.add_message(format!("📨 Convite gerado: {}", invite_uri), "Sistema".into());
                            app.status_message = "Aguardando conexão...".to_string();

                            if let Err(e) = network.start_host(addr, pubkey_bytes).await {
                                app.add_message(format!("❌ Erro ao iniciar host: {}", e), "Sistema".into());
                            }
                        }
                        Action::ConnectTo(uri) => {
                            if let Ok(parsed_uri) = url::Url::parse(&uri) {
                                let their_pubkey_hex = parsed_uri.query_pairs()
                                    .find_map(|(key, value)| if key == "pubkey" { Some(value) } else { None })
                                    .ok_or("Chave pública não encontrada na URI");

                                if let Ok(hex) = their_pubkey_hex {
                                    match hex::decode(hex.as_ref()) {
                                        Ok(their_pubkey_bytes) => {
                                            if their_pubkey_bytes.len() == 32 {
                                                let their_public_key = PublicKey::from(
                                                    <[u8; 32]>::try_from(their_pubkey_bytes).unwrap()
                                                );

                                                let (secret, public) = generate_keypair();
                                                app.local_fingerprint = Some(crypton::get_fingerprint(&public));
                                                app.remote_fingerprint = Some(crypton::get_fingerprint(&their_public_key));

                                                app.add_message(
                                                    format!("🔑 Seu fingerprint X25519: {}", app.local_fingerprint.as_ref().unwrap()),
                                                    "Sistema".into()
                                                );
                                                app.add_message(
                                                    format!("🔑 Fingerprint do par X25519: {}", app.remote_fingerprint.as_ref().unwrap()),
                                                    "Sistema".into()
                                                );

                                                secret_key = Some(secret);
                                                // Deixa para criar o ratchet_session quando peer conectar
                                                // (não podemos consumir secret aqui)

                                                if let Err(e) = network.connect_to_host(&uri, public.to_bytes()).await {
                                                    app.add_message(format!("❌ Erro de conexão: {}", e), "Sistema".into());
                                                }
                                            } else {
                                                app.add_message("❌ Chave pública inválida (tamanho incorreto)".to_string(), "Sistema".into());
                                            }
                                        }
                                        Err(_) => {
                                            app.add_message("❌ Erro ao decodificar chave pública".to_string(), "Sistema".into());
                                        }
                                    }
                                }
                            }
                        }
                        Action::SendMessage(msg) => {
                            if let Some(session) = &mut ratchet_session {
                                let chat_msg = ChatMessage {
                                    sender: app.username.clone(),
                                    content: msg.clone()
                                };
                                let plaintext = serde_json::to_vec(&chat_msg).unwrap();

                                // Adiciona padding para ofuscar tamanho
                                let padded = add_padding(&plaintext);

                                // Criptografa com ratchet (PFS + proteção replay)
                                match session.encrypt(&padded) {
                                    Ok(ratchet_msg) => {
                                        let encrypted_bytes = ratchet_msg.to_bytes();
                                        if network.send_message(encrypted_bytes).await.is_ok() {
                                            app.add_message(msg, "Você".to_string());
                                        } else {
                                            app.status_message = "Falha ao enviar mensagem".to_string();
                                        }
                                    }
                                    Err(_) => app.status_message = "Erro de criptografia".to_string(),
                                }
                            }
                        }
                        Action::SetUsername(name) => {
                            app.username = name;
                            app.status_message = format!("Nome de usuário alterado para: {}", app.username);
                        }
                    }
                }
            }
            Event::Key(key) => {
                app.handle_key(key)?;
            }
            Event::Tick => {
                app.tick();
            }
            Event::Network(net_event) => {
                match net_event {
                    network_secure::NetworkEvent::PeerConnected { public_key, ed25519_key, fingerprint } => {
                        if let Some(sk) = secret_key.take() {
                            let their_pk = PublicKey::from(public_key);

                            // Exibe fingerprints de ambas as identidades
                            app.add_message(
                                format!("✓ Par conectado!"),
                                "Sistema".into()
                            );
                            app.add_message(
                                format!("🔐 Identidade Ed25519 do par: {}", fingerprint),
                                "Sistema".into()
                            );

                            let shared_secret = sk.diffie_hellman(&their_pk);
                            ratchet_session = Some(RatchetSession::new(shared_secret.as_bytes()));
                            app.mode = AppMode::Connected;
                            app.status_message = "Conexão segura e autenticada estabelecida!".to_string();

                            // Armazena fingerprints para verificação
                            app.remote_fingerprint = Some(fingerprint);
                        }
                    }
                    network_secure::NetworkEvent::DataReceived(data) => {
                        if let Some(session) = &mut ratchet_session {
                            // Converte bytes para RatchetMessage
                            match ratchet::RatchetMessage::from_bytes(&data) {
                                Ok(ratchet_msg) => {
                                    // Descriptografa com verificação de replay
                                    match session.decrypt(&ratchet_msg) {
                                        Ok(padded_data) => {
                                            // Remove padding
                                            match remove_padding(&padded_data) {
                                                Ok(plaintext) => {
                                                    if let Ok(msg) = serde_json::from_slice::<ChatMessage>(&plaintext) {
                                                        app.add_message(msg.content, msg.sender);
                                                    }
                                                }
                                                Err(_) => app.add_message(
                                                    "❌ Erro ao remover padding".to_string(),
                                                    "Sistema".into()
                                                ),
                                            }
                                        }
                                        Err(e) => app.add_message(
                                            format!("❌ {}", e),
                                            "Sistema".into()
                                        ),
                                    }
                                }
                                Err(_) => app.add_message(
                                    "❌ Formato de mensagem inválido".to_string(),
                                    "Sistema".into()
                                ),
                            }
                        }
                    }
                    network_secure::NetworkEvent::PeerDisconnected => {
                        app.mode = AppMode::Menu;
                        app.status_message = "Par desconectado.".to_string();
                        ratchet_session = None;
                        app.remote_fingerprint = None;
                    }
                    network_secure::NetworkEvent::ConnectionEstablished => {
                        app.status_message = "Estabelecendo handshake autenticado...".to_string();
                    }
                    network_secure::NetworkEvent::ConnectionFailed(err) => {
                        app.mode = AppMode::Menu;
                        app.status_message = format!("❌ Falha na conexão: {}", err);
                        app.add_message(format!("❌ {}", err), "Sistema".into());
                    }
                    network_secure::NetworkEvent::Log(msg) => {
                        app.add_message(msg, "Sistema".into());
                    }
                    network_secure::NetworkEvent::FingerprintVerificationRequired { fingerprint, ed25519_key } => {
                        app.add_message(
                            format!("⚠️  VERIFICAÇÃO NECESSÁRIA!"),
                            "AVISO".into()
                        );
                        app.add_message(
                            format!("Fingerprint Ed25519: {}", fingerprint),
                            "AVISO".into()
                        );
                        app.add_message(
                            "VERIFIQUE por um canal seguro (telefone, pessoalmente, etc)".to_string(),
                            "AVISO".into()
                        );
                    }
                }
            }
            _ => {}
        }
    }

    tui.restore()?;
    Ok(())
}
