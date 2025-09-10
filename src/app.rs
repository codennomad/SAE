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
pub enum Action {
    GenerateInvite,
    ConnectTo(String),
    SendMessage(String),
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
    pub scroll_offset: usize, // Para scroll das mensagens
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
            status_message: "Welcome to SAE - Secure Anonymous Echo".to_string(),
            invite_uri: None,
            qr_code: None,
            scroll_offset: 0,
        }
    }

    pub fn tick(&mut self) {
        self.last_tick = Instant::now();
        
        // Banner timeout - mais rápido
        if let Some(banner_start) = self.banner_time {
            if banner_start.elapsed().as_secs() >= 2 {
                self.banner_time = None;
                self.state = AppState::Menu;
            }
        }
        
        // TEMPO DAS MENSAGENS AUMENTADO
        let now = Instant::now();
        self.messages.retain_mut(|msg| {
            let elapsed = now.duration_since(msg.arrival_time).as_millis();
            match msg.state {
                MessageState::FadingIn if elapsed > 300 => {
                    msg.state = MessageState::Glitching;
                }
                MessageState::Glitching if elapsed > 700 => {
                    msg.state = MessageState::Visible;
                }
                // TEMPO AUMENTADO: 60 segundos ao invés de 10
                MessageState::Visible if elapsed > 60000 => {
                    msg.state = MessageState::FadingOut;
                }
                // TEMPO AUMENTADO: 65 segundos total
                MessageState::FadingOut if elapsed > 65000 => {
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
                // Skip banner on any key
                self.banner_time = None;
                self.state = AppState::Menu;
            }
            _ => {
                match key.code {
                    KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.should_quit = true;
                    }
                    // Scroll para cima
                    KeyCode::PageUp => {
                        if self.scroll_offset > 0 {
                            self.scroll_offset = self.scroll_offset.saturating_sub(5);
                        }
                    }
                    // Scroll para baixo
                    KeyCode::PageDown => {
                        if self.scroll_offset < self.messages.len().saturating_sub(1) {
                            self.scroll_offset += 5;
                        }
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
                    KeyCode::Delete => {
                        if self.input_cursor < self.input.len() {
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
                    KeyCode::Home => {
                        self.input_cursor = 0;
                    }
                    KeyCode::End => {
                        self.input_cursor = self.input.len();
                    }
                    KeyCode::Enter => {
                        // Handled in main.rs
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

    pub fn handle_input(&mut self) -> Result<Option<Action>> {
        let input = self.input.trim().to_string(); 
        self.input.clear();
        self.input_cursor = 0;
        
        if input.is_empty() {
            return Ok(None);
        }

        if input.starts_with('/') {
            self.handle_command(&input)
        } else {
            // Auto-scroll para mostrar mensagens mais recentes
            self.scroll_offset = 0;
            Ok(Some(Action::SendMessage(input)))
        }
    }

    fn handle_command(&mut self, command: &str) -> Result<Option<Action>> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        
        match parts.first() {
            Some(&"/quit") | Some(&"/q") | Some(&"/exit") => {
                self.should_quit = true;
                Ok(None)
            }
            Some(&"/invite") | Some(&"/i") => {
                self.mode = AppMode::Host;
                self.status_message = "[NET] >> Generating secure channel...".to_string();
                Ok(Some(Action::GenerateInvite))
            }
            Some(&"/connect") | Some(&"/c") => {
                if let Some(uri) = parts.get(1) {
                    self.mode = AppMode::Client;
                    self.status_message = format!("[NET] >> Establishing link to: {}", uri);
                    Ok(Some(Action::ConnectTo(uri.to_string())))
                } else {
                    self.status_message = "[ERR] >> Invalid syntax: /connect <sae://uri>".to_string();
                    Ok(None)
                }
            }
            Some(&"/clear") => {
                self.messages.clear();
                self.scroll_offset = 0;
                self.status_message = "[SYS] >> Data logs purged".to_string();
                Ok(None)
            }
            Some(&"/scroll") => {
                if let Some(pos) = parts.get(1) {
                    if pos == &"top" {
                        self.scroll_offset = self.messages.len().saturating_sub(1);
                    } else if pos == &"bottom" {
                        self.scroll_offset = 0;
                    }
                }
                Ok(None)
            }
            Some(&"/help") | Some(&"/h") => {
                self.add_system_message("[MANUAL] >> Available commands: /invite, /connect <uri>, /clear, /scroll [top|bottom], /quit");
                self.add_system_message("[MANUAL] >> Navigation: [PgUp/PgDn] scroll logs | [Ctrl+Q] emergency exit");
                Ok(None)
            }
            _ => {
                self.status_message = format!("[ERR] >> Unknown command: {} (use /help)", command);
                Ok(None)
            }
        }
    }

    pub fn add_message(&mut self, content: String, sender: Option<String>) {
        let message = DisplayMessage::new(content, sender);
        self.messages.push(message);
        
        // Auto-scroll para mostrar novas mensagens
        self.scroll_offset = 0;
        
        // Limita o número máximo de mensagens (performance)
        if self.messages.len() > 1000 {
            self.messages.remove(0);
        }
    }

    pub fn add_system_message(&mut self, content: &str) {
        self.add_message(content.to_string(), Some("System".to_string()));
    }

    // Método auxiliar para obter mensagens visíveis com scroll
    pub fn get_visible_messages(&self, max_lines: usize) -> Vec<&DisplayMessage> {
        let total = self.messages.len();
        if total == 0 {
            return Vec::new();
        }
        
        let start_idx = if self.scroll_offset >= total {
            0
        } else {
            total.saturating_sub(self.scroll_offset + max_lines)
        };
        
        let end_idx = if self.scroll_offset == 0 {
            total
        } else {
            total.saturating_sub(self.scroll_offset)
        };
        
        self.messages[start_idx..end_idx].iter().collect()
    }
}