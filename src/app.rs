use color_eyre::eyre::Result;
use crossterm::event::KeyEvent;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Menu,
    Host,
    Client,
    Connected,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MessageState {
    FadingIn,
    Glitching,
    Visible,
    FadingOut,
}

#[derive(Debug, Clone)]
pub struct DisplayMessage {
    pub content: String,
    pub arrival_time: Instant,
    pub state: MessageState,
    pub sender: Option<String>,
}

impl DisplayMessage {
    pub fn new(content: String, sender: Option<String>) -> Self {
        Self {
            content,
            arrival_time: Instant::now(),
            state: MessageState::FadingIn,
            sender,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppState {
    Banner,
    Menu,
    WaitingForConnection,
    Connected,
    Disconnected,
}

/// Estado principal da aplicação
pub struct App {
    pub should_quit: bool,
    pub mode: AppMode,
    pub state: AppState,
    pub messages: Vec<DisplayMessage>,
    pub input: String,
    pub input_cursor: usize,
    pub last_tick: Instant,
    pub banner_time: Option<Instant>,
    pub status_message: String,
    pub invite_uri: Option<String>,
    pub qr_code: Option<String>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            should_quit: false,
            mode: AppMode::Menu,
            state: AppState::Banner,
            messages: Vec::new(),
            input: String::new(),
            input_cursor: 0,
            last_tick: Instant::now(),
            banner_time: Some(Instant::now()),
            status_message: "Welcome to SAE".to_string(),
            invite_uri: None,
            qr_code: None,
        }
    }

    pub fn tick(&mut self) {
        self.last_tick = Instant::now();
        
        // Lida com timeout do banner
        if let Some(banner_start) = self.banner_time {
            if banner_start.elapsed().as_secs() >= 3 {
                self.banner_time = None;
                self.state = AppState::Menu;
            }
        }
        
        // Atualiza estados das mensagens
        let now = Instant::now();
        self.messages.retain_mut(|msg| {
            let elapsed = now.duration_since(msg.arrival_time).as_millis();
            match msg.state {
                MessageState::FadingIn if elapsed > 200 => {
                    msg.state = MessageState::Glitching;
                }
                MessageState::Glitching if elapsed > 500 => {
                    msg.state = MessageState::Visible;
                }
                MessageState::Visible if elapsed > 10000 => {
                    // 10 segundos
                    msg.state = MessageState::FadingOut;
                }
                MessageState::FadingOut if elapsed > 11000 => {
                    return false; // Remove mensagem
                }
                _ => {}
            }
            true
        });
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        use crossterm::event::{KeyCode, KeyModifiers};
        
        match self.state {
            AppState::Banner => {
                // Qualquer tecla durante banner pula para menu
                self.banner_time = None;
                self.state = AppState::Menu;
            }
            _ => {
                match key.code {
                    KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.should_quit = true;
                    }
                    KeyCode::Char(c) => {
                        self.input.insert(self.input_cursor, c);
                        self.input_cursor += 1;
                    }
                    KeyCode::Backspace => {
                        if self.input_cursor > 0 {
                            self.input_cursor -= 1;
                            self.input.remove(self.input_cursor);
                        }
                    }
                    KeyCode::Left => {
                        if self.input_cursor > 0 {
                            self.input_cursor -= 1;
                        }
                    }
                    KeyCode::Right => {
                        if self.input_cursor < self.input.len() {
                            self.input_cursor += 1;
                        }
                    }
                    KeyCode::Enter => {
                        self.handle_input()?;
                    }
                    KeyCode::Esc => {
                        self.input.clear();
                        self.input_cursor = 0;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn handle_input(&mut self) -> Result<()> {
        let input = self.input.trim().to_string(); // Clona o input para evitar problemas de borrow
        if input.is_empty() {
            return Ok(());
        }

        if input.starts_with('/') {
            self.handle_command(&input)?;
        } else {
            // Mensagem regular
            self.add_message(input, Some("You".to_string()));
        }

        self.input.clear();
        self.input_cursor = 0;
        Ok(())
    }

    fn handle_command(&mut self, command: &str) -> Result<()> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        
        match parts.first() {
            Some(&"/quit") | Some(&"/q") | Some(&"/exit") => {
                self.should_quit = true;
            }
            Some(&"/invite") | Some(&"/i") => {
                self.mode = AppMode::Host;
                self.status_message = "Generating invitation...".to_string();
                // TODO: Generate actual invite
                self.invite_uri = Some("sae://example_key@127.0.0.1:8080?token=abc123".to_string());
            }
            Some(&"/connect") | Some(&"/c") => {
                if let Some(uri) = parts.get(1) {
                    self.mode = AppMode::Client;
                    self.status_message = format!("Connecting to {}", uri);
                    // TODO: Initiate connection
                } else {
                    self.status_message = "Usage: /connect <sae://uri>".to_string();
                }
            }
            Some(&"/clear") => {
                self.messages.clear();
            }
            Some(&"/help") | Some(&"/h") => {
                self.add_system_message("Commands: /invite, /connect <uri>, /clear, /quit");
            }
            _ => {
                self.status_message = format!("Unknown command: {}", command);
            }
        }
        Ok(())
    }

    pub fn add_message(&mut self, content: String, sender: Option<String>) {
        let message = DisplayMessage::new(content, sender);
        self.messages.push(message);
    }

    pub fn add_system_message(&mut self, content: &str) {
        self.add_message(content.to_string(), Some("System".to_string()));
    }
}