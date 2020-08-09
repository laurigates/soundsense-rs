use crate::app::App;
use crate::message::Threshold;

use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Tabs, Block, Borders, Gauge, List, ListItem, Paragraph},
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
            let threshold = Paragraph::new(Span::from(threshold_label))
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
                .gauge_style(Style::default().fg(color).bg(Color::Black))
                .label(Span::from(channel_label))
                .percent(channel.volume as u16);
            f.render_widget(gauge, chunks[1]);
        }
    }
    {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
            .split(chunks[1]);
        let titles = app
            .tabs
            .titles
            .iter()
            .map(|t| {
                let (first, rest) = t.split_at(1);
                Spans::from(vec![
                    Span::styled(first, Style::default().fg(Color::Yellow)),
                    Span::styled(rest, Style::default().fg(Color::Green)),
                ])
            })
            .collect();
        let tabs = Tabs::new(titles)
            .block(Block::default().borders(Borders::ALL).title("Logs"))
            .select(app.tabs.index)
            .style(Style::default().fg(Color::Cyan))
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .bg(Color::Black),
            );
        f.render_widget(tabs, chunks[0]);
        match app.tabs.index {
            0 => {
                let items: Vec<ListItem> = app
                    .items
                    .iter()
                    .map(|i| {
                        let lines = Spans::from(i.to_string());
                        ListItem::new(lines)
                    })
                    .collect();
                let items = List::new(items).block(Block::default().borders(Borders::ALL));
                f.render_widget(items, chunks[1])
            }
            1 => {
                let gamelog: Vec<ListItem> = app
                    .gamelog
                    .iter()
                    .map(|i| {
                        let color = match i.channel.as_str() {
                            "music" => Color::Magenta,
                            "weather" => Color::Blue,
                            "feelings" => Color::LightMagenta,
                            "trade" => Color::Yellow,
                            "combat" => Color::Red,
                            _ => Color::Reset,
                        };
                        let line = Span::styled(i.text.to_string(), Style::default().fg(color));
                        ListItem::new(line)
                    })
                    .collect();
                let gamelog = List::new(gamelog).block(Block::default().borders(Borders::ALL));
                f.render_widget(gamelog, chunks[1])
            }
            _ => unreachable!(),
        };
    }
}
