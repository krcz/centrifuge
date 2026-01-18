use polyepoxide_llm::{ContentBlock, MessageContent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

use super::app::{AppMode, ChatApp};

pub fn render(frame: &mut Frame, app: &ChatApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header
            Constraint::Min(1),    // Messages
            Constraint::Length(3), // Input
            Constraint::Length(1), // Status bar
        ])
        .split(frame.area());

    render_header(frame, app, chunks[0]);
    render_messages(frame, app, chunks[1]);
    render_input(frame, app, chunks[2]);
    render_status_bar(frame, app, chunks[3]);

    // Render popup if active
    match app.mode {
        AppMode::SelectModel => render_model_popup(frame, app),
        AppMode::SelectReasoning => render_reasoning_popup(frame, app),
        _ => {}
    }
}

fn render_header(frame: &mut Frame, app: &ChatApp, area: Rect) {
    let reasoning_text = match &app.reasoning_effort {
        Some(r) => format!(" [reasoning: {}]", r),
        None => String::new(),
    };

    let cid_text = match app.conversation_cid() {
        Some(cid) => format!("  CID: {}", cid),
        None => String::new(),
    };

    let title = format!("sih chat - {}{}{}", app.model, reasoning_text, cid_text);

    let header = Paragraph::new(title).style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

    frame.render_widget(header, area);
}

fn render_messages(frame: &mut Frame, app: &ChatApp, area: Rect) {
    let messages = app.get_messages();
    let mut lines: Vec<Line> = Vec::new();

    for msg in messages {
        let (role, style, content_blocks) = match &msg.content {
            MessageContent::User(blocks) => ("User", Style::default().fg(Color::Green), blocks.as_slice()),
            MessageContent::Assistant { blocks, .. } => {
                let model_name = msg
                    .metadata
                    .as_ref()
                    .and_then(|m| m.model.as_ref())
                    .map(|m| m.as_str())
                    .unwrap_or("Assistant");
                (model_name, Style::default().fg(Color::Blue), blocks.as_slice())
            }
            MessageContent::System(blocks) => ("System", Style::default().fg(Color::Yellow), blocks.as_slice()),
            MessageContent::ToolResult { .. } => continue, // Skip tool results in display
        };

        // Role header
        lines.push(Line::from(Span::styled(format!("{}:", role), style.add_modifier(Modifier::BOLD))));

        // Content blocks
        for block in content_blocks {
            match block {
                ContentBlock::Text(text) => {
                    for line in text.lines() {
                        lines.push(Line::from(format!("  {}", line)));
                    }
                }
                ContentBlock::Thinking(text) => {
                    lines.push(Line::from(Span::styled(
                        "  [Thinking]",
                        Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
                    )));
                    for line in text.lines() {
                        lines.push(Line::from(Span::styled(
                            format!("  {}", line),
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                }
                ContentBlock::Code { language, code } => {
                    let lang = language.as_deref().unwrap_or("code");
                    lines.push(Line::from(Span::styled(
                        format!("  ```{}", lang),
                        Style::default().fg(Color::Magenta),
                    )));
                    for line in code.lines() {
                        lines.push(Line::from(Span::styled(
                            format!("  {}", line),
                            Style::default().fg(Color::White),
                        )));
                    }
                    lines.push(Line::from(Span::styled("  ```", Style::default().fg(Color::Magenta))));
                }
                ContentBlock::Image(_) => {
                    lines.push(Line::from(Span::styled(
                        "  [Image]",
                        Style::default().fg(Color::DarkGray),
                    )));
                }
                ContentBlock::File { name, .. } => {
                    lines.push(Line::from(Span::styled(
                        format!("  [File: {}]", name),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            }
        }

        lines.push(Line::from("")); // Empty line between messages
    }

    // Loading indicator
    if app.mode == AppMode::Loading {
        lines.push(Line::from(Span::styled(
            "Waiting for response...",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::ITALIC),
        )));
    }

    // Error display
    if let Some(ref error) = app.last_error {
        lines.push(Line::from(Span::styled(
            format!("Error: {}", error),
            Style::default().fg(Color::Red),
        )));
    }

    let messages_block = Block::default().borders(Borders::ALL).title("Messages");

    // Calculate scroll offset to show the bottom of the conversation
    let visible_height = area.height.saturating_sub(2) as usize; // Account for borders
    let total_lines = lines.len();
    let scroll = if total_lines > visible_height {
        (total_lines - visible_height).saturating_sub(app.messages_scroll as usize)
    } else {
        0
    };

    let paragraph = Paragraph::new(Text::from(lines))
        .block(messages_block)
        .wrap(Wrap { trim: false })
        .scroll((scroll as u16, 0));

    frame.render_widget(paragraph, area);
}

fn render_input(frame: &mut Frame, app: &ChatApp, area: Rect) {
    let input_block = Block::default().borders(Borders::ALL).title("Input");

    let display_text = if app.input.is_empty() {
        "Type your message here...".to_string()
    } else {
        app.input.clone()
    };

    let style = if app.input.is_empty() {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    };

    let input_paragraph = Paragraph::new(display_text)
        .style(style)
        .block(input_block);

    frame.render_widget(input_paragraph, area);

    // Set cursor position if in chat mode
    if app.mode == AppMode::Chat && !app.input.is_empty() {
        let cursor_x = area.x + 1 + app.cursor_pos as u16;
        let cursor_y = area.y + 1;
        frame.set_cursor_position((cursor_x, cursor_y));
    } else if app.mode == AppMode::Chat {
        frame.set_cursor_position((area.x + 1, area.y + 1));
    }
}

fn render_status_bar(frame: &mut Frame, app: &ChatApp, area: Rect) {
    let status = match app.mode {
        AppMode::Chat => "Enter: Send  F2: Model  F3: Reasoning  Ctrl+↑/↓: Scroll  Esc: Quit",
        AppMode::Loading => "Waiting for response...  Esc: Cancel",
        AppMode::SelectModel | AppMode::SelectReasoning => "↑/↓: Navigate  Enter: Select  Esc: Cancel",
    };

    let status_bar = Paragraph::new(status).style(Style::default().fg(Color::DarkGray));

    frame.render_widget(status_bar, area);
}

fn render_popup(frame: &mut Frame, title: &str, items: Vec<ListItem>, selected: usize) {
    let area = centered_rect(40, 50, frame.area());

    frame.render_widget(Clear, area);

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .highlight_symbol("> ");

    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(selected));

    frame.render_stateful_widget(list, area, &mut state);
}

fn render_model_popup(frame: &mut Frame, app: &ChatApp) {
    let items: Vec<ListItem> = ChatApp::available_models()
        .iter()
        .map(|model| {
            let style = if *model == app.model {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };
            let marker = if *model == app.model { " ✓" } else { "" };
            ListItem::new(format!("{}{}", model, marker)).style(style)
        })
        .collect();

    render_popup(frame, "Select Model", items, app.popup_selected);
}

fn render_reasoning_popup(frame: &mut Frame, app: &ChatApp) {
    let items: Vec<ListItem> = ChatApp::reasoning_options()
        .iter()
        .map(|opt| {
            let label = opt.unwrap_or("None");
            let is_selected = *opt == app.reasoning_effort.as_deref();
            let style = if is_selected {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };
            let marker = if is_selected { " ✓" } else { "" };
            ListItem::new(format!("{}{}", label, marker)).style(style)
        })
        .collect();

    render_popup(frame, "Select Reasoning Effort", items, app.popup_selected);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
