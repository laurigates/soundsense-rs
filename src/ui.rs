use crate::app::App;
use crate::message::Threshold;

use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Gauge, List, Text},
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
            let mut color = Color::LightGreen;
            // Hightlight selected item
            if app.channels.state.selected() == Some(i) {
                color = Color::Red
            }
            let mut channel_label = channel.name.to_string();
            if channel.paused {
                channel_label.push_str("(paused)")
            }
            let threshold_label = match channel.threshold {
                Threshold::Nothing => "(threshold: nothing)",
                Threshold::Critical => "(threshold: critical)",
                Threshold::Important => "(threshold: important)",
                Threshold::Fluff => "(threshold: fluff)",
                Threshold::Everything => "(threshold: everything)",
            };
            channel_label.push_str(threshold_label);
            let gauge = Gauge::default()
                .style(Style::default().fg(color).bg(Color::Black))
                .label(&channel_label)
                .percent(channel.volume as u16);
            f.render_widget(gauge, chunks[i]);
        }
    }
    {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .margin(0)
            .constraints([Constraint::Percentage(100)].as_ref())
            .split(chunks[1]);

        let items = app.items.iter().map(Text::raw);
        let items = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Log"))
            .style(Style::default().fg(Color::Green))
            .highlight_style(
                Style::default()
                    .fg(Color::LightGreen)
                    .modifier(Modifier::BOLD),
            );
        f.render_widget(items, chunks[0])
    }
}
