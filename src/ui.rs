use crate::app::{App, AppState, DisplayMessage, MessageState};
use ratatui::{
    layout::{Constraint, Layout, Margin},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
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
    
    // Cria texto do banner glitched
    let banner_text = if let Some(banner_start) = app.banner_time {
        let elapsed = banner_start.elapsed().as_millis();
        if elapsed < 1000 {
            create_glitched_text("SAE", elapsed)
        } else {
            "SAE\nSecure Anonymous Echo".to_string()
        }
    } else {
        "SAE".to_string()
    };
    
    let banner = Paragraph::new(banner_text)
        .style(Style::default().fg(Color::Rgb(189, 0, 255))) // Cyberpunk purple
        .block(Block::default().borders(Borders::NONE))
        .wrap(Wrap { trim: true });
    
    // Centraliza o banner
    let vertical = Layout::vertical([
        Constraint::Percentage(40),
        Constraint::Min(3),
        Constraint::Percentage(40),
    ]);
    let [_, center, _] = vertical.areas(area);
    
    let horizontal = Layout::horizontal([
        Constraint::Percentage(25),
        Constraint::Min(20),
        Constraint::Percentage(25),
    ]);
    let [_, banner_area, _] = horizontal.areas(center);
    
    frame.render_widget(banner, banner_area);
}

fn render_main_interface(frame: &mut Frame, app: &App) {
    let area = frame.area();
    
    // Layout principal: área de mensagens, barra de status, área de entrada
    let layout = Layout::vertical([
        Constraint::Min(3),         // Área de mensagens
        Constraint::Length(1),      // Barra de status  
        Constraint::Length(3),      // Área de entrada
    ]);
    let [messages_area, status_area, input_area] = layout.areas(area);
    
    // Renderiza mensagens
    render_messages(frame, app, messages_area);
    
    // Renderiza barra de status
    render_status_bar(frame, app, status_area);
    
    // Renderiza área de entrada
    render_input(frame, app, input_area);
}

fn render_messages(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let mut lines = Vec::new();
    
    for message in &app.messages {
        let message_lines = create_message_lines(message, app);
        lines.extend(message_lines);
    }
    
    if lines.is_empty() && app.state == AppState::Menu {
        lines.push(Line::from(vec![
            Span::styled("Welcome to SAE - Secure Anonymous Echo",
                Style::default().fg(Color::Rgb(189, 0, 255))),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Commands:",
                Style::default().fg(Color::Rgb(0, 240, 255))),
        ]));
        lines.push(Line::from(" /invite - Create an invitation"));
        lines.push(Line::from(" /connect <uri> - Connect to a host"));
        lines.push(Line::from(" /help - Show help"));
        lines.push(Line::from(" /quit - Exit"));
    }
    
    let messages_widget = Paragraph::new(lines)
        .block(Block::default()
            .title("Messages")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(0, 255, 159))))
        .wrap(Wrap { trim: true });
    
    frame.render_widget(messages_widget, area);
}

fn render_status_bar(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let status_text = format!(" {} | Mode: {:?} ", app.status_message, app.mode);
    let status = Paragraph::new(status_text)
        .style(Style::default()
            .bg(Color::Rgb(40, 40, 40))
            .fg(Color::Rgb(0, 240, 255)));
    
    frame.render_widget(status, area);
}

fn render_input(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let input_widget = Paragraph::new(app.input.as_str())
        .block(Block::default()
            .title("Input")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(0, 255, 159))));
    
    frame.render_widget(input_widget, area);
    
    // Define posição do cursor
    frame.set_cursor_position((
        area.x + app.input_cursor as u16 + 1,
        area.y + 1,
    ));
}

fn create_message_lines(message: &DisplayMessage, _app: &App) -> Vec<Line<'static>> {
    let sender_prefix = if let Some(sender) = &message.sender {
        format!("[{}] ", sender)
    } else {
        String::new()
    };
    
    let content = format!("{}{}", sender_prefix, message.content);
    
    // Aplica estilo baseado no estado da mensagem
    let style = match message.state {
        MessageState::FadingIn => {
            let elapsed = message.arrival_time.elapsed().as_millis();
            let alpha = (elapsed as f32 / 200.0).min(1.0);
            let brightness = (255.0 * alpha) as u8;
            Style::default().fg(Color::Rgb(brightness, brightness, brightness))
        }
        MessageState::Glitching => {
            Style::default().fg(Color::Rgb(255, 0, 60)) // Glitch red
        }
        MessageState::Visible => {
            Style::default().fg(Color::White)
        }
        MessageState::FadingOut => {
            let elapsed = message.arrival_time.elapsed().as_millis();
            let fade_start = 10000; // Começa a desvanecer após 10 segundos
            let fade_progress = ((elapsed - fade_start) as f32 / 1000.0).min(1.0);
            let brightness = (255.0 * (1.0 - fade_progress)) as u8;
            Style::default().fg(Color::Rgb(brightness, brightness, brightness))
        }
    };
    
    let display_content = if message.state == MessageState::Glitching {
        glitch_text(&content, message.arrival_time.elapsed().as_millis())
    } else {
        content
    };
    
    vec![Line::from(Span::styled(display_content, style))]
}

fn create_glitched_text(text: &str, elapsed_ms: u128) -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let glitch_chars = "█▓▒░▄▀■□▪▫";
    let mut result = String::new();
    
    for c in text.chars() {
        if elapsed_ms < 500 && rng.gen_bool(0.3) {
            let glitch_char = glitch_chars.chars()
                .nth(rng.gen_range(0..glitch_chars.len()))
                .unwrap_or(c);
            result.push(glitch_char);
        } else {
            result.push(c);
        }
    }
    
    if elapsed_ms < 800 {
        result.push('\n');
        for _ in 0..20 {
            if rng.gen_bool(0.1) {
                let glitch_char = glitch_chars.chars()
                    .nth(rng.gen_range(0..glitch_chars.len()))
                    .unwrap_or(' ');
                result.push(glitch_char);
            } else {
                result.push(' ');
            }
        }
    }
    
    result
}

fn glitch_text(text: &str, elapsed_ms: u128) -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let glitch_chars = "█▓▒░";
    let mut result = String::new();
    
    for c in text.chars() {
        if elapsed_ms < 300 && rng.gen_bool(0.15) {
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
