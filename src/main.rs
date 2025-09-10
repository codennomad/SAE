use color_eyre::eyre::Result;
use tokio::time::Duration;
use tokio::sync::mpsc;
use std::net::SocketAddr;
use app::{App, AppMode, Action};

mod app;
mod crypton;
mod event;
mod network;
mod tui;
mod ui;

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

                if key.code == crossterm::event::KeyCode::Enter {
                    if let Some(action) = app.handle_input()? {
                        match action {
                            Action::GenerateInvite => {
                                println!("🔗 Starting host...");
                                let addr: SocketAddr = "0.0.0.0:8080".parse().unwrap();
                                match network.start_host(addr).await {
                                    Ok(invite_uri) => {
                                        app.invite_uri = Some(invite_uri.clone());
                                        app.status_message =
                                            "✅ Host started! Share the invite URI".to_string();
                                        app.add_system_message(&format!(
                                            "📋 Invite URI: {}",
                                            invite_uri
                                        ));
                                        app.add_system_message(
                                            "💡 Share this URI with others to connect",
                                        );

                                        println!("✅ Host started successfully");
                                        println!("📋 Invite URI: {}", invite_uri);
                                    }
                                    Err(e) => {
                                        app.status_message =
                                            format!("❌ Failed to start host: {}", e);
                                        app.add_system_message(&format!("Error: {}", e));
                                        eprintln!("❌ Host error: {}", e);
                                    }
                                }
                            }
                            Action::ConnectTo(uri) => {
                                println!("🔌 Attempting to connect to: {}", uri);
                                app.add_system_message(&format!("🔌 Connecting to {}", uri));
                                match network.connect_to_host(&uri).await {
                                    Ok(()) => {
                                        app.status_message = "🔄 Connecting...".to_string();
                                        println!("✅ Connection initiated");
                                    }
                                    Err(e) => {
                                        app.status_message =
                                            format!("❌ Connection failed: {}", e);
                                        app.add_system_message(&format!("Connection error: {}", e));
                                        eprintln!("❌ Connection error: {}", e);
                                        app.mode = AppMode::Menu;
                                    }
                                }
                            }
                            Action::SendMessage(msg) => {
                                if app.mode == AppMode::Connected {
                                    app.add_message(msg.clone(), Some("You".to_string()));
                                    if let Err(e) = network.send_message(&msg).await {
                                        app.status_message =
                                            format!("❌ Send failed: {}", e);
                                        app.add_system_message(&format!("Send error: {}", e));
                                    } else {
                                        println!("📨 Message sent: {}", msg);
                                    }
                                } else {
                                    app.status_message =
                                        "❌ Not connected to anyone".to_string();
                                    app.add_system_message(
                                        "You need to be connected to send messages",
                                    );
                                }
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
                app.status_message = format!("[TERM] >> Display matrix: {}x{}", w, h);
            }
        }
    }

    // Cleanup
    tui.restore()?;
    println!("[SYS] >> Neural interface terminated. Phantom network disconnected.");
    Ok(())
}

fn handle_network_event(app: &mut App, event: NetworkEvent) {
    match event {
        NetworkEvent::MessageReceived(msg) => {
            println!("[NET] >> Incoming data packet: {}", msg);
            app.add_message(msg, Some("Remote".to_string()));
            app.status_message = "[NET] >> Data transmission received".to_string();
        }
        NetworkEvent::UserConnected(addr) => {
            println!("[NET] >> Phantom node connected: {}", addr);
            app.add_system_message(&format!(
                "[NET] >> Anonymous user linked from: {}",
                addr
            ));
            app.status_message = "[LINK] >> Phantom connection established".to_string();
            app.mode = AppMode::Connected;
        }
        NetworkEvent::UserDisconnected(addr) => {
            println!("[NET] >> Phantom node disconnected: {}", addr);
            app.add_system_message(&format!(
                "[NET] >> Anonymous user severed link: {}",
                addr
            ));
            if app.mode == AppMode::Connected {
                app.mode = AppMode::Menu;
                app.status_message = "[LINK] >> Connection terminated".to_string();
            }
        }
        NetworkEvent::ConnectionEstablished => {
            println!("[NET] >> Phantom handshake completed");
            app.add_system_message(
                "[NET] >> Secure tunnel established! Phantom protocol active",
            );
            app.status_message = "[LINK] >> Neural interface synchronized".to_string();
            app.mode = AppMode::Connected;
        }
        NetworkEvent::ConnectionFailed(error) => {
            println!("[NET] >> Connection failed: {}", error);
            app.add_system_message(&format!("[ERROR] >> Phantom link failed: {}", error));
            app.status_message = "[NET] >> Connection terminated".to_string();
            app.mode = AppMode::Menu;
        }
        NetworkEvent::InviteGenerated(uri) => {
            println!("[NET] >> Phantom invite generated: {}", uri);
            app.invite_uri = Some(uri.clone());
            app.add_system_message(
                "[NET] >> Phantom channel initialized. Broadcasting secure invitation...",
            );
            app.add_system_message(
                "[INFO] >> Share the above phantom link to establish encrypted neural bridge",
            );
        }
    }
}
