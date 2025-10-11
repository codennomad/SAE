use color_eyre::eyre::Result;
use crossterm::event::KeyEvent;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Modos de operação da aplicação.
#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Menu,
    Host,
    Client,
    Connected,
}

/// Estado da animação de uma mensagem.
#[derive(Debug, Clone, PartialEq)]
pub enum MessageState {
    FadingIn,
    Visible,
    FadingOut,
}

/// Ações que podem ser disparadas pela UI.
#[derive(Debug, Clone)]
pub enum Action {
    GenerateInvite,
    ConnectTo(String),
    SendMessage(String),
    SetUsername(String),
}

/// Representa uma mensagem de chat a ser serializada e enviada.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub sender: String,
    pub content: String,
}

/// Representa uma mensagem para exibição na TUI.
#[derive(Debug, Clone)]
pub struct DisplayMessage {
    pub content: String,
    pub arrival_time: Instant,
    pub state: MessageState,
    pub sender: String,
}

impl DisplayMessage {
    pub fn new(content: String, sender: String) -> Self {
        Self {
            content,
            arrival_time: Instant::now(),
            state: MessageState::FadingIn,
            sender,
        }
    }
}

/// Estado geral da aplicação.
pub struct App {
    pub should_quit: bool,
    pub mode: AppMode,
    pub messages: Vec<DisplayMessage>,
    pub input: String,
    pub status_message: String,
    pub username: String,
    pub local_fingerprint: Option<String>,
    pub remote_fingerprint: Option<String>,
    // Adicione outros campos de estado conforme necessário
}

impl App {
    pub fn new() -> Self {
        Self {
            should_quit: false,
            mode: AppMode::Menu,
            messages: Vec::new(),
            input: String::new(),
            status_message: "Bem-vindo ao SAE - Secure Anonymous Echo".to_string(),
            username: "Phantom".to_string(),
            local_fingerprint: None,
            remote_fingerprint: None,
        }
    }

    /// Chamado a cada "tick" do loop principal para atualizar o estado.
    pub fn tick(&mut self) {
        let now = Instant::now();
        self.messages.retain_mut(|msg| {
            let elapsed = now.duration_since(msg.arrival_time).as_millis();
            match msg.state {
                MessageState::FadingIn if elapsed > 500 => msg.state = MessageState::Visible,
                MessageState::Visible if elapsed > 60000 => msg.state = MessageState::FadingOut, // 1 minuto
                MessageState::FadingOut if elapsed > 61000 => return false, // Remove após 1s de fade-out
                _ => {}
            }
            true
        });
    }

    /// Processa a entrada do teclado.
    pub fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        use crossterm::event::{KeyCode, KeyModifiers};

        match key.code {
            KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                self.should_quit = true;
            }
            KeyCode::Char(c) => {
                self.input.push(c);
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Enter => {
                // O manuseio do Enter é feito no loop principal para disparar ações.
            }
            KeyCode::Esc => {
                self.input.clear();
            }
            _ => {}
        }
        Ok(())
    }

    /// Processa a entrada do usuário após o Enter.
    pub fn handle_input(&mut self) -> Result<Option<Action>> {
        let input = self.input.trim().to_string();
        self.input.clear();

        if input.is_empty() {
            return Ok(None);
        }

        if input.starts_with('/') {
            self.handle_command(&input)
        } else {
            if self.mode == AppMode::Connected {
                Ok(Some(Action::SendMessage(input)))
            } else {
                self.status_message = "Não conectado. Use /invite ou /connect.".to_string();
                Ok(None)
            }
        }
    }

    /// Processa comandos que começam com '/'.
    fn handle_command(&mut self, command: &str) -> Result<Option<Action>> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        match parts.first() {
            Some(&"/quit") => {
                self.should_quit = true;
                Ok(None)
            }
            Some(&"/invite") => {
                self.mode = AppMode::Host;
                self.status_message = "Gerando convite seguro...".to_string();
                Ok(Some(Action::GenerateInvite))
            }
            Some(&"/connect") => {
                if let Some(uri) = parts.get(1) {
                    self.mode = AppMode::Client;
                    self.status_message = format!("Conectando a {}...", uri);
                    Ok(Some(Action::ConnectTo(uri.to_string())))
                } else {
                    self.status_message = "Uso: /connect <sae://uri>".to_string();
                    Ok(None)
                }
            }
            Some(&"/nick") => {
                if let Some(new_name) = parts.get(1) {
                    Ok(Some(Action::SetUsername(new_name.to_string())))
                } else {
                    self.status_message = "Uso: /nick <username>".to_string();
                    Ok(None)
                }
            }
            _ => {
                self.status_message = format!("Comando desconhecido: {}", command);
                Ok(None)
            }
        }
    }

    /// Adiciona uma mensagem à lista de exibição.
    pub fn add_message(&mut self, content: String, sender: String) {
        let message = DisplayMessage::new(content, sender);
        self.messages.push(message);
    }
}
