//! TUI rendering with ratatui.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use tui_tree_widget::Tree;

use crate::app::App;

/// Render the TUI.
pub fn render(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header/breadcrumb
            Constraint::Min(5),    // Tree
            Constraint::Length(3), // Help bar
        ])
        .split(frame.area());

    render_header(frame, app, chunks[0]);
    render_tree(frame, app, chunks[1]);
    render_help(frame, chunks[2]);
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let path = app.tree.breadcrumb_path();
    let title = format!(" Polyepoxide Explorer - {} ", path);

    let block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::White))
        .title(title);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Show selected node info
    let selected_node = app
        .tree_state
        .selected()
        .last()
        .and_then(|id| app.tree.get_node(id));

    if let Some(node) = selected_node {
        let info = format!(
            "Type: {}{}",
            node.type_hint,
            node.cid
                .map(|c| format!("  CID: {}", c))
                .unwrap_or_default()
        );
        let para = Paragraph::new(info).style(Style::default().fg(Color::Cyan));
        frame.render_widget(para, inner);
    }
}

fn render_tree(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Structure ");

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let items = app.tree.tree_items();
    let tree = Tree::new(&items)
        .expect("unique identifiers")
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .node_closed_symbol("▶ ")
        .node_open_symbol("▼ ")
        .node_no_children_symbol("  ");

    frame.render_stateful_widget(tree, inner, &mut app.tree_state);
}

fn render_help(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let help_spans = vec![
        Span::styled("↑↓", Style::default().fg(Color::Yellow)),
        Span::raw(" Navigate  "),
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::raw(" Expand  "),
        Span::styled("z", Style::default().fg(Color::Yellow)),
        Span::raw(" Zoom  "),
        Span::styled("s", Style::default().fg(Color::Yellow)),
        Span::raw(" Schema  "),
        Span::styled("b", Style::default().fg(Color::Yellow)),
        Span::raw(" Back  "),
        Span::styled("e", Style::default().fg(Color::Yellow)),
        Span::raw(" Export JSON  "),
        Span::styled("y", Style::default().fg(Color::Yellow)),
        Span::raw(" Export YAML  "),
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::raw(" Quit"),
    ];

    let help = Paragraph::new(Line::from(help_spans));
    frame.render_widget(help, inner);
}
