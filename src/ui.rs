use crate::app::App;
use crate::message::Threshold;

use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Gauge, List, Paragraph, Text},
    Frame,
};

pub fn draw<B: Backend>(app: &App, f: &mut Frame<B>) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("soundsense-rs");
    f.render_widget(block, f.size());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
        .split(f.size());

    {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints(
                [
                    Constraint::Ratio(1, 6),
                    Constraint::Ratio(1, 6),
                    Constraint::Ratio(1, 6),
                    Constraint::Ratio(1, 6),
                    Constraint::Ratio(1, 6),
                    Constraint::Ratio(1, 6),
                ]
                .as_ref(),
            )
            .split(chunks[0]);

        for (i, channel) in app.channels.items.iter().enumerate() {
            let chunks = Layout::default()
                .constraints([Constraint::Percentage(15), Constraint::Percentage(85)].as_ref())
                .direction(Direction::Horizontal)
                .split(chunks[i]);

            let mut threshold_label = "threshold: ".to_string();

            threshold_label.push_str(match channel.threshold {
                Threshold::Nothing => "nothing",
                Threshold::Critical => "critical",
                Threshold::Important => "important",
                Threshold::Fluff => "fluff",
                Threshold::Everything => "everything",
            });
            let lines = [Text::raw(threshold_label)];
            let threshold = Paragraph::new(lines.iter())
                .style(Style::default().fg(Color::LightGreen).bg(Color::Black));
            f.render_widget(threshold, chunks[0]);

            let mut color = Color::LightGreen;
            // Hightlight selected item
            if app.channels.state.selected() == Some(i) {
                color = Color::Red
            }
            let mut channel_label = channel.name.to_string();
            if channel.paused {
                channel_label.push_str("(paused)")
            }
            let gauge = Gauge::default()
                .style(Style::default().fg(color).bg(Color::Black))
                .label(&channel_label)
                .percent(channel.volume as u16);
            f.render_widget(gauge, chunks[1]);
        }
    }
    {
        let items = app.items.iter().map(Text::raw);
        let items = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Log"))
            .style(Style::default().fg(Color::Green))
            .highlight_style(
                Style::default()
                    .fg(Color::LightGreen)
                    .modifier(Modifier::BOLD),
            );
        f.render_widget(items, chunks[1])
    }
}
