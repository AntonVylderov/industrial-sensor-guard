use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Terminal,
};
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use tokio::sync::broadcast;
use std::collections::HashMap;
use std::io::stdout;
use crate::models::{SensorReading, AggregatedReading};

fn get_status_style(value: f64, warning: f64, critical: f64) -> Span<'static> {
    if value >= critical {
        Span::styled("CRITICAL", Style::new().fg(Color::Red).bold())
    } else if value >= warning {
        Span::styled("WARNING", Style::new().fg(Color::Yellow).bold())
    } else {
        Span::styled("NORMAL", Style::new().fg(Color::Green).bold())
    }
}

pub async fn run(
    mut rx: broadcast::Receiver<SensorReading>,
    rules: HashMap<String, (f64, f64)>,
    mut rx_agg: broadcast::Receiver<AggregatedReading>,
) -> anyhow::Result<()> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(ratatui::backend::CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    let mut data: HashMap<String, (f64, String)> = HashMap::new();   // сырые
    let mut agg_data: HashMap<String, (f64, String)> = HashMap::new(); // агрегированные
    let mut lagged_counter: u64 = 0;

    loop {
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    break;
                }
            }
        }

        // Чтение сырых сообщений
        while let Ok(reading) = rx.try_recv() {
            data.insert(reading.parameter.clone(), (reading.value, reading.unit));
        }
        // Чтение агрегированных сообщений
        while let Ok(agg) = rx_agg.try_recv() {
            agg_data.insert(agg.parameter.clone(), (agg.value, agg.unit));
        }

        // Обработка ошибок отставания (Lagged) – только для сырого канала, для агрегатов аналогично
        match rx.try_recv() {
            Err(broadcast::error::TryRecvError::Lagged(skipped)) => {
                lagged_counter += skipped as u64;
                tracing::warn!("UI lagged, skipped {} raw messages (total: {})", skipped, lagged_counter);
            }
            _ => {}
        }

        terminal.draw(|frame| {
            let area = frame.area();
            let main_layout = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(10),
                    Constraint::Length(3),
                ])
                .split(area);

            let header = Paragraph::new(vec![
                Line::from("🏭 Industrial Sensor Monitor".bold().fg(Color::Cyan)),
                Line::from("Real-time monitoring dashboard".dim()),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::new().fg(Color::Cyan))
                    .title("Dashboard"),
            );
            frame.render_widget(header, main_layout[0]);

            let table_area = main_layout[1];
            // Собираем все параметры из сырых данных
            let mut rows: Vec<Row> = Vec::new();
            for (param, (raw_value, unit)) in data.iter() {
                let agg_value = agg_data.get(param).map(|(v, _)| *v).unwrap_or(*raw_value);
                let (warning, critical) = rules.get(param).unwrap_or(&(0.0, 0.0));
                let status_span = get_status_style(*raw_value, *warning, *critical);
                rows.push(Row::new(vec![
                    Cell::from(param.clone()),
                    Cell::from(format!("{:.2} {}", raw_value, unit)),
                    Cell::from(format!("{:.2} {}", agg_value, unit)),
                    Cell::from(status_span),
                ]));
            }

            let table = Table::new(
                rows,
                [
                    Constraint::Percentage(30),
                    Constraint::Percentage(25),
                    Constraint::Percentage(25),
                    Constraint::Percentage(20),
                ],
            )
            .header(
                Row::new(vec![
                    Cell::from("Parameter").style(Style::new().bold().fg(Color::Blue)),
                    Cell::from("Raw").style(Style::new().bold().fg(Color::Blue)),
                    Cell::from("Aggregated").style(Style::new().bold().fg(Color::Blue)),
                    Cell::from("Status (Raw)").style(Style::new().bold().fg(Color::Blue)),
                ])
                .bottom_margin(1),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::new().fg(Color::Blue))
                    .title("📊 Sensor Readings"),
            )
            .column_spacing(1);
            frame.render_widget(table, table_area);

            let footer_text = if lagged_counter > 0 {
                format!(
                    "Press Ctrl+C to exit | Total sensors: {} | Skipped messages: {}",
                    data.len(), lagged_counter
                )
            } else {
                format!("Press Ctrl+C to exit | Total sensors: {}", data.len())
            };
            let footer = Paragraph::new(vec![Line::from(footer_text.dim())])
                .block(Block::default().borders(Borders::ALL).border_style(Style::new().fg(Color::Gray)));
            frame.render_widget(footer, main_layout[2]);
        })?;

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}