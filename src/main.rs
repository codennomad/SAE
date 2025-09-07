use color_eyre::eyre::Result;
use tokio::time::Duration;
use tokio::sync::mpsc;
use std::net::SocketAddr;

mod app;
mod crypton;
mod event;
mod network;
mod tui;
mod ui;

use app::{App, AppMode};
use event::{Event, EventHandler};
use network::{NetworkManager, NetworkEvent};
use tui::TuiManager;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    
    let mut app = App::new();
    let mut tui = TuiManager::new()?;
    
    // Inicializa terminal
    tui.init()?;
    
    // Cria manipulador de eventos
    let mut events = EventHandler::new(Duration::from_millis(50));
    
    // Cria canal de eventos de rede
    let (network_sender, mut network_receiver) = mpsc::unbounded_channel::<NetworkEvent>();
    
    // Cria gerenciador de rede
    let mut network = NetworkManager::new(network_sender);
    
    // Pega sender de eventos para eventos de rede
    let event_sender = events.sender();
    
    // Spawn network event forwarder
    tokio::spawn(async move {
        while let Some(net_event) = network_receiver.recv().await {
            let _ = event_sender.send(Event::Network(net_event));
        }
    });
    
    // Loop principal da aplicação
    while !app.should_quit {
        // Desenha o estado atual
        tui.draw(&mut app)?;
        
        // Lida com eventos
        match events.next().await? {
            Event::Key(key) => {
                app.handle_key(key)?;
                
                // Lida com comandos relacionados à rede
                if let Some(command) = check_network_commands(&app) {
                    match command {
                        NetworkCommand::StartHost => {
                            let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
                            match network.start_host(addr).await {
                                Ok(invite_uri) => {
                                    app.invite_uri = Some(invite_uri.clone());
                                    app.status_message = "Host started! Invite URI generated.".to_string();
                                    app.add_system_message(&format!("Invite URI: {}", invite_uri));
                                }
                                Err(e) => {
                                    app.status_message = format!("Failed to start host: {}", e);
                                }
                            }
                        }
                        NetworkCommand::ConnectToHost(uri) => {
                            match network.connect_to_host(&uri).await {
                                Ok(()) => {
                                    app.status_message = "Connecting...".to_string();
                                }
                                Err(e) => {
                                    app.status_message = format!("Connection failed: {}", e);
                                }
                            }
                        }
                        NetworkCommand::SendMessage(msg) => {
                            if let Err(e) = network.send_message(&msg).await {
                                app.status_message = format!("Failed to send message: {}", e);
                            }
                        }
                    }
                }
            }
            Event::Tick => {
                app.tick();
                // Limpa tokens expirados periodicamente
                network.cleanup_expired_tokens().await;
            }
            Event::Network(net_event) => {
                handle_network_event(&mut app, net_event);
            }
            Event::Resize(w, h) => {
                // Lida com redimensionamento de terminal se necessário
                app.status_message = format!("Terminal resized to {}x{}", w, h);
            }
        }
    }
    
    // Cleanup
    tui.restore()?;
    Ok(())
}

#[derive(Debug)]
enum NetworkCommand {
    StartHost,
    ConnectToHost(String),
    SendMessage(String),
}

fn check_network_commands(app: &App) -> Option<NetworkCommand> {
    // Esta é uma verificação simplificada - em uma implementação real, você desejaria
    // rastrear o estado do comando de forma mais cuidadosa
    match app.mode {
        AppMode::Host if app.invite_uri.is_none() => Some(NetworkCommand::StartHost),
        _ => None,
    }
}

fn handle_network_event(app: &mut App, event: NetworkEvent) {
    match event {
        NetworkEvent::MessageReceived(msg) => {
            app.add_message(msg, Some("Remote".to_string()));
        }
        NetworkEvent::UserConnected(addr) => {
            app.add_system_message(&format!("User connected: {}", addr));
            app.status_message = "Connected".to_string();
            app.mode = AppMode::Connected;
        }
        NetworkEvent::UserDisconnected(addr) => {
            app.add_system_message(&format!("User disconnected: {}", addr));
            if app.mode == AppMode::Connected {
                app.mode = AppMode::Menu;
                app.status_message = "Disconnected".to_string();
            }
        }
        NetworkEvent::ConnectionEstablished => {
            app.add_system_message("Connected to host!");
            app.status_message = "Connected".to_string();
            app.mode = AppMode::Connected;
        }
        NetworkEvent::ConnectionFailed(error) => {
            app.add_system_message(&format!("Connection failed: {}", error));
            app.status_message = "Connection failed".to_string();
            app.mode = AppMode::Menu;
        }
        NetworkEvent::InviteGenerated(uri) => {
            app.invite_uri = Some(uri.clone());
            app.add_system_message("Invite generated! Share the URI or QR code to connect.");
        }
    }
}