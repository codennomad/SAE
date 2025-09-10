use crate::app::{App, AppState, DisplayMessage, MessageState};
use ratatui::{
    layout::{Constraint, Layout, Margin, Alignment},
    style::{Color, Style, Modifier},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap, Gauge, Clear},
    Frame,
};

pub fn ui(frame: &mut Frame, app: &App) {
    match app.state {
        AppState::Banner => {
            render_banner(frame, app);
        }
        _ => {
            render_main_interface(frame, app);
        }
    }
}

fn render_banner(frame: &mut Frame, app: &App) {
    let area = frame.area();
    
    // Banner com animação mais suave
    let banner_text = if let Some(banner_start) = app.banner_time {
        let elapsed = banner_start.elapsed().as_millis();
        create_smooth_banner("SAE", elapsed)
    } else {
        "SAE".to_string()
    };
    
    let banner = Paragraph::new(banner_text)
        .style(Style::default()
            .fg(Color::Rgb(147, 112, 219)) // Roxo mais suave
            .add_modifier(Modifier::BOLD))
        .block(Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(100, 149, 237)))
            .title(" SAE - Secure Anonymous Echo "))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    
    // Layout centralizado
    let vertical = Layout::vertical([
        Constraint::Percentage(30),
        Constraint::Min(5),
        Constraint::Percentage(30),
    ]);
    let [_, center, _] = vertical.areas(area);
    
    let horizontal = Layout::horizontal([
        Constraint::Percentage(20),
        Constraint::Min(40),
        Constraint::Percentage(20),
    ]);
    let [_, banner_area, _] = horizontal.areas(center);
    
    frame.render_widget(banner, banner_area);
}

fn render_main_interface(frame: &mut Frame, app: &App) {
    let area = frame.area();
    
    // Layout principal melhorado
    let layout = Layout::vertical([
        Constraint::Min(5),         // Mensagens (área principal)
        Constraint::Length(3),      // Status e info
        Constraint::Length(3),      // Input
    ]);
    let [messages_area, status_area, input_area] = layout.areas(area);
    
    // Renderiza cada seção
    render_messages(frame, app, messages_area);
    render_status_section(frame, app, status_area);
    render_input_section(frame, app, input_area);
}

fn render_messages(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let mut lines = Vec::new();
    
    // Calcula quantas linhas cabem na área
    let available_lines = area.height.saturating_sub(2) as usize; // -2 para as bordas
    let visible_messages = app.get_visible_messages(available_lines);
    
    if visible_messages.is_empty() && app.state == AppState::Menu {
        lines.extend(create_welcome_screen());
    } else {
        // Corrigido: usar referência para não mover o valor
        for message in &visible_messages {
            let message_lines = create_message_lines(message);
            lines.extend(message_lines);
        }
    }
    
    // Indicador de scroll se há mais mensagens
    let title = if app.messages.len() > available_lines {
        format!(" Messages ({}/{}) - PgUp/PgDn to scroll ", 
                visible_messages.len(), app.messages.len())
    } else {
        " Messages ".to_string()
    };
    
    let messages_widget = Paragraph::new(lines)
        .block(Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(get_border_style(&app.mode)))
        .wrap(Wrap { trim: true });
    
    frame.render_widget(messages_widget, area);
}

fn render_status_section(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let layout = Layout::horizontal([
        Constraint::Percentage(70),  // Status message
        Constraint::Percentage(30),  // Connection info
    ]);
    let [status_area, info_area] = layout.areas(area);
    
    // Status message
    let status = Paragraph::new(app.status_message.clone())
        .style(Style::default()
            .fg(Color::Rgb(0, 255, 255))
            .add_modifier(Modifier::ITALIC))
        .block(Block::default()
            .borders(Borders::ALL)
            .title(" [STATUS] "));
    
    // Connection info
    let (mode_text, mode_color) = match app.mode {
        crate::app::AppMode::Menu => ("[STANDBY]", Color::Rgb(85, 85, 85)),
        crate::app::AppMode::Host => ("[HOST]", Color::Rgb(0, 255, 65)),
        crate::app::AppMode::Client => ("[CLIENT]", Color::Rgb(255, 255, 0)),
        crate::app::AppMode::Connected => ("[PHANTOM]", Color::Rgb(0, 255, 255)),
    };
    
    let info = Paragraph::new(format!("NODE: {}", mode_text))
        .style(Style::default()
            .fg(mode_color)
            .add_modifier(Modifier::BOLD))
        .block(Block::default()
            .borders(Borders::ALL)
            .title(" [NODE] "));
    
    frame.render_widget(status, status_area);
    frame.render_widget(info, info_area);
}

fn render_input_section(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let input_text = if app.input.is_empty() {
        match app.mode {
            crate::app::AppMode::Menu => ">> Enter command or data transmission...",
            crate::app::AppMode::Connected => ">> Neural interface ready for input...",
            _ => ">> System busy. Please wait...",
        }
    } else {
        &app.input
    };
    
    let input_style = if app.input.is_empty() {
        Style::default().fg(Color::Rgb(85, 85, 85)).add_modifier(Modifier::ITALIC)
    } else {
        Style::default().fg(Color::Rgb(0, 255, 65))
    };
    
    let input_widget = Paragraph::new(input_text)
        .style(input_style)
        .block(Block::default()
            .title(" [INPUT] ")
            .borders(Borders::ALL)
            .border_style(get_border_style(&app.mode)));
    
    frame.render_widget(input_widget, area);
    
    // Cursor apenas se há texto real
    if !app.input.is_empty() {
        frame.set_cursor_position((
            area.x + app.input_cursor as u16 + 1,
            area.y + 1,
        ));
    }
}

fn create_welcome_screen() -> Vec<Line<'static>> {
    vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("Welcome to SAE - Secure Anonymous Echo",
                Style::default()
                    .fg(Color::Rgb(147, 112, 219))
                    .add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Quick Start:",
                Style::default()
                    .fg(Color::Rgb(100, 149, 237))
                    .add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  /invite", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(" - Create an invitation link", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  /connect <uri>", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(" - Connect to a host", Style::default().fg(Color::White)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Commands:",
                Style::default()
                    .fg(Color::Rgb(255, 165, 0))
                    .add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  /help", Style::default().fg(Color::Cyan)),
            Span::styled(" - Show all commands", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("  /clear", Style::default().fg(Color::Cyan)),
            Span::styled(" - Clear messages", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("  /quit", Style::default().fg(Color::Red)),
            Span::styled(" - Exit application", Style::default().fg(Color::Gray)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Navigation: PgUp/PgDn to scroll, Ctrl+Q to quit",
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)),
        ]),
    ]
}

fn create_message_lines(message: &DisplayMessage) -> Vec<Line<'static>> {
    let sender_style = match message.sender.as_deref() {
        Some("System") => Style::default().fg(Color::Rgb(255, 165, 0)).add_modifier(Modifier::BOLD),
        Some("You") => Style::default().fg(Color::Rgb(100, 149, 237)).add_modifier(Modifier::BOLD),
        Some("Remote") => Style::default().fg(Color::Rgb(144, 238, 144)).add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::Gray),
    };
    
    let content_style = get_message_style(message);
    let content = get_message_content(message);
    
    let mut spans = Vec::new();
    
    // Timestamp
    let timestamp = format!("[{}] ", 
        message.arrival_time.elapsed().as_secs());
    spans.push(Span::styled(timestamp, Style::default().fg(Color::DarkGray)));
    
    // Sender
    if let Some(sender) = &message.sender {
        spans.push(Span::styled(format!("{}: ", sender), sender_style));
    }
    
    // Content
    spans.push(Span::styled(content, content_style));
    
    vec![Line::from(spans)]
}

fn get_message_style(message: &DisplayMessage) -> Style {
    match message.state {
        MessageState::FadingIn => {
            let elapsed = message.arrival_time.elapsed().as_millis();
            let alpha = (elapsed as f32 / 300.0).min(1.0);
            let brightness = (200.0 * alpha + 55.0) as u8; // Min brightness 55
            Style::default().fg(Color::Rgb(brightness, brightness, brightness))
        }
        MessageState::Glitching => {
            Style::default()
                .fg(Color::Rgb(255, 100, 100))
                .add_modifier(Modifier::RAPID_BLINK)
        }
        MessageState::Visible => {
            Style::default().fg(Color::White)
        }
        MessageState::FadingOut => {
            let elapsed = message.arrival_time.elapsed().as_millis();
            let fade_start = 60000;
            let fade_progress = ((elapsed - fade_start) as f32 / 5000.0).min(1.0);
            let brightness = (255.0 * (1.0 - fade_progress)).max(50.0) as u8;
            Style::default().fg(Color::Rgb(brightness, brightness, brightness))
        }
    }
}

fn get_message_content(message: &DisplayMessage) -> String {
    if message.state == MessageState::Glitching {
        apply_glitch_effect(&message.content, message.arrival_time.elapsed().as_millis())
    } else {
        message.content.clone()
    }
}

fn get_border_style(mode: &crate::app::AppMode) -> Style {
    match mode {
        crate::app::AppMode::Menu => Style::default().fg(Color::Rgb(85, 85, 85)),
        crate::app::AppMode::Host => Style::default().fg(Color::Rgb(0, 255, 65)), // Matrix green
        crate::app::AppMode::Client => Style::default().fg(Color::Rgb(255, 255, 0)), // Warning yellow
        crate::app::AppMode::Connected => Style::default().fg(Color::Rgb(0, 255, 255)), // Cyan
    }
}

fn create_smooth_banner(text: &str, elapsed_ms: u128) -> String {
    match elapsed_ms {
        0..=300 => {
            // Static phase - Matrix-like
            "███ ▓▓▓ ░░░\n█▓░ ▓▓▓ ░░░\n▓▓▓ ███ ▓▓▓".to_string()
        }
        301..=600 => {
            // Partial reveal
            "█▓░ █▓░ ░░░\nSAE ▓▓▓ ░░░\n▓▓▓ ███ ▓▓▓".to_string()
        }
        601..=900 => {
            format!("{}\n>>> PHANTOM NETWORK <<<", text)
        }
        901..=1200 => {
            format!("{}\n>>> PHANTOM NETWORK <<<\n[ESTABLISHING SECURE LINK...]", text)
        }
        1201..=1500 => {
            format!("{}\n>>> PHANTOM NETWORK <<<\n[ESTABLISHING SECURE LINK...]\n[NEURAL INTERFACE ACTIVE]", text)
        }
        _ => {
            format!("{}\n>>> PHANTOM NETWORK <<<\n[ESTABLISHING SECURE LINK...]\n[NEURAL INTERFACE ACTIVE]\n\n>> PRESS ANY KEY TO JACK IN <<", text)
        }
    }
}

fn apply_glitch_effect(text: &str, elapsed_ms: u128) -> String {
    if elapsed_ms > 400 {
        return text.to_string();
    }
    
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let glitch_chars = "█▓▒░◢◣◤◥▲▼◆◇";
    let mut result = String::new();
    
    for c in text.chars() {
        if rng.gen_bool(0.15) { // 15% chance de glitch - mais intenso
            let glitch_char = glitch_chars.chars()
                .nth(rng.gen_range(0..glitch_chars.len()))
                .unwrap_or(c);
            result.push(glitch_char);
        } else {
            result.push(c);
        }
    }
    result
}