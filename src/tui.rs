use crate::app::{App, AppMode, MessageState};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .constraints([
            Constraint::Min(0),    // Área de mensagens
            Constraint::Length(3), // Barra de status e fingerprints
            Constraint::Length(3), // Caixa de entrada
        ])
        .split(f.area());

    render_messages(f, app, chunks[0]);
    render_status_bar(f, app, chunks[1]);
    render_input_box(f, app, chunks[2]);
}

fn render_messages(f: &mut Frame, app: &mut App, area: Rect) {
    let messages: Vec<Line> = app.messages.iter().map(|msg| {
        let sender_style = match msg.sender.as_str() {
            "Sistema" => Style::default().fg(Color::Yellow),
            "AVISO" => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            "Você" => Style::default().fg(Color::Cyan),
            _ => Style::default().fg(Color::Green),
        };
        
        let content_style = match msg.state {
            MessageState::FadingIn => Style::default().fg(Color::DarkGray),
            MessageState::Visible => Style::default().fg(Color::White),
            MessageState::FadingOut => Style::default().fg(Color::Gray),
        };

        Line::from(vec![
            Span::styled(format!("[{}] ", msg.sender), sender_style),
            Span::styled(&msg.content, content_style),
        ])
    }).collect();

    let messages_paragraph = Paragraph::new(messages)
        .block(Block::default().borders(Borders::ALL).title("Log de Transmissão"))
        .wrap(Wrap { trim: true })
        .scroll((app.messages.len().saturating_sub(area.height as usize - 2) as u16, 0));

    f.render_widget(messages_paragraph, area);
}

fn render_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let mode_text = match app.mode {
        AppMode::Menu => "Menu",
        AppMode::Host => "Host",
        AppMode::Client => "Cliente",
        AppMode::Connected => "Conectado",
    };
    
    let status_line = Line::from(vec![
        Span::styled(format!("Modo: {} | ", mode_text), Style::default().fg(Color::Green)),
        Span::raw(&app.status_message),
    ]);

    let your_fp = app.local_fingerprint.as_deref().unwrap_or("N/A");
    let their_fp = app.remote_fingerprint.as_deref().unwrap_or("N/A");

    let fp_line = Line::from(vec![
        Span::styled("Seu FP: ", Style::default().fg(Color::Cyan)),
        Span::raw(your_fp),
        Span::raw(" | "),
        Span::styled("FP do Par: ", Style::default().fg(Color::Green)),
        Span::raw(their_fp),
    ]);
    
    let status_paragraph = Paragraph::new(vec![status_line, fp_line])
        .block(Block::default().borders(Borders::TOP));

    f.render_widget(status_paragraph, area);
}

fn render_input_box(f: &mut Frame, app: &App, area: Rect) {
    let prompt = format!("{}> ", app.username);
    let input_text = format!("{}{}", prompt, app.input);

    let input_paragraph = Paragraph::new(input_text)
        .block(Block::default().borders(Borders::ALL).title("Comando"));
    
    f.render_widget(input_paragraph, area);
    f.set_cursor_position((
        area.x + prompt.len() as u16 + app.input.len() as u16 + 1,
        area.y + 1,
    ));
}
