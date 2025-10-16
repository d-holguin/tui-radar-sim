use crate::fps_counter::FpsCounter;
use crate::radar::RadarWidget;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::prelude::Color;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Terminal, crossterm};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

pub type MyResult<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Clone)]
pub enum Message {
    Quit,
    Tick,
    Render,
    KeyPress(KeyCode),
}

#[derive(Clone, Debug)]
pub enum UpdateCommand {
    None,
    Quit,
}

pub struct Model {
    pub fps_counter: FpsCounter,
    pub radar: RadarWidget,
    pub last_spawn_time: Instant,
    pub sweep_rate: f64,
    pub next_id: u32,
}

pub struct Tui {
    pub terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
    pub frame_rate: f64,
    pub tick_rate: f64,
    pub msg_tx: mpsc::Sender<Message>,
    pub msg_rx: mpsc::Receiver<Message>,
    pub model: Model,
}

impl Tui {
    pub fn new(frame_rate: f64, tick_rate: f64) -> MyResult<Self> {
        let terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
        let (msg_tx, msg_rx) = mpsc::channel();

        let sweep_rate = RadarWidget::DEGREES_PER_SECOND / 6.0;

        let fade_duration = sweep_rate * 1.75;

        let mut radar = RadarWidget::new(1000.0, fade_duration);

        radar.spawn_aircraft(1);
        radar.spawn_ship(100);
        radar.spawn_unknown(200);
        radar.spawn_hostile(300);
        radar.spawn_generic(400);
        radar.spawn_weather(500);

        radar.spawn_aircraft(2);
        radar.spawn_ship(101);

        Ok(Self {
            terminal,
            frame_rate,
            tick_rate,
            msg_tx,
            msg_rx,
            model: Model {
                fps_counter: FpsCounter::new(),
                radar,
                last_spawn_time: Instant::now(),
                sweep_rate,
                next_id: 1000,
            },
        })
    }

    fn enter(&self) -> MyResult<()> {
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(std::io::stdout(), EnterAlternateScreen)?;
        Ok(())
    }

    pub fn exit(&mut self) -> MyResult<()> {
        if crossterm::terminal::is_raw_mode_enabled()? {
            self.terminal.flush()?;
            crossterm::execute!(std::io::stdout(), LeaveAlternateScreen)?;
            crossterm::terminal::disable_raw_mode()?;
            self.terminal.show_cursor()?;
            println!("Terminal exited.");
        }
        Ok(())
    }

    pub fn run(&mut self) -> MyResult<()> {
        self.enter()?;

        let tick_duration = Duration::from_secs_f64(1.0 / self.tick_rate);
        let frame_duration = Duration::from_secs_f64(1.0 / self.frame_rate);

        let now = Instant::now();
        let mut last_tick = now;
        let mut last_frame = now;

        // Spawn input thread
        let input_tx = self.msg_tx.clone();
        thread::spawn(move || {
            // This thread blocks safely on input and sends key events to main thread
            loop {
                if let Ok(Event::Key(key)) = crossterm::event::read() {
                    if key.kind == KeyEventKind::Press {
                        if input_tx.send(Message::KeyPress(key.code)).is_err() {
                            break; // main thread exited
                        }
                    }
                }
            }
        });

        // main thread loop
        loop {
            // Handle incoming messages (non-blocking)
            while let Ok(msg) = self.msg_rx.try_recv() {
                match self.update(&msg)? {
                    UpdateCommand::Quit => {
                        self.exit()?;
                        return Ok(());
                    }
                    _ => {}
                }
            }

            let now = Instant::now();

            // Tick logic
            if now >= last_tick + tick_duration {
                self.update(&Message::Tick)?;
                last_tick += tick_duration;
            }

            // Render frame
            if now >= last_frame + frame_duration {
                self.update(&Message::Render)?;
                last_frame += frame_duration;
            }
            // sleep until next event, yield to CPU
            let next_tick = last_tick + tick_duration;
            let next_frame = last_frame + frame_duration;
            let next_event = std::cmp::min(next_tick, next_frame);
            let sleep_time = next_event.saturating_duration_since(Instant::now());

            thread::sleep(sleep_time);
        }
    }

    fn update(&mut self, message: &Message) -> MyResult<UpdateCommand> {
        match message {
            Message::Quit => {
                return Ok(UpdateCommand::None);
            }
            Message::KeyPress(key) => match key {
                KeyCode::Esc | KeyCode::Char('q') => {
                    return Ok(UpdateCommand::Quit);
                }
                _ => {}
            },
            Message::Tick => {
                let delta_time = 1.0 / self.tick_rate;
                let now = Instant::now();
                self.model.radar.update_world_objects(delta_time);
                self.model.radar.update_sweep(delta_time);

                // Spawn diverse traffic
                if now.duration_since(self.model.last_spawn_time).as_secs() >= 5 {
                    let id = self.model.next_id;
                    self.model.next_id += 1;

                    // Spawn different types with different frequencies
                    match id % 10 {
                        0..=3 => self.model.radar.spawn_aircraft(id), // 40% aircraft
                        4..=5 => self.model.radar.spawn_ship(id),     // 20% ships
                        6 => self.model.radar.spawn_unknown(id),      // 10% unknown
                        7 => self.model.radar.spawn_hostile(id),      // 10% hostile
                        8 => self.model.radar.spawn_generic(id),      // 10% generic
                        9 => self.model.radar.spawn_weather(id),      // 10% weather
                        _ => self.model.radar.spawn_random_object(id),
                    }

                    self.model.last_spawn_time = now;
                }
            }
            Message::Render => {
                self.model.fps_counter.tick();
                self.view()?;
            }
        }
        Ok(UpdateCommand::None)
    }

    fn view(&mut self) -> MyResult<()> {
        self.terminal.draw(|f| {
            let main_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(80), // Radar
                    Constraint::Percentage(20), // Controls
                ])
                .split(f.area());

            // Radar display (80%)
            f.render_widget(&self.model.radar, main_chunks[0]);

            // Control panel (20%)split horizontally into 4 sections
            let control_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(25), // System info
                    Constraint::Percentage(25), // Target info
                    Constraint::Percentage(25), // Legend
                    Constraint::Percentage(25), // Controls
                ])
                .split(main_chunks[1]);

            // System info panel (cleaner without legend)
            let system_text = Text::from(vec![
                Line::from(format!("FPS: {}", self.model.fps_counter.fps)),
                Line::from(format!("Range: {} nm", self.model.radar.max_range)),
                Line::from(format!("Sweep Rate: {:.1} RPM", self.model.sweep_rate)),
                Line::from(""),
                Line::from("System Status:"),
                Line::styled("● Online", Style::default().fg(Color::Green)),
                Line::styled("● Tracking", Style::default().fg(Color::Green)),
            ]);

            let system_info = Paragraph::new(system_text)
                .block(Block::default().borders(Borders::ALL).title("System"));
            f.render_widget(system_info, control_chunks[0]);

            // Target info panel
            let target_info = Paragraph::new(format!(
                "Contacts: {}\n\nAlerts: 0\n\nNearest:\n--:-- nm\n\nFarthest:\n--:-- nm",
                self.model.radar.detected_contacts.len(),
            ))
            .block(Block::default().borders(Borders::ALL).title("Contacts"));
            f.render_widget(target_info, control_chunks[1]);

            // Legend panel
            let legend_text = Text::from(vec![
                Line::styled("^ Aircraft", Style::default().fg(Color::Cyan).bold()),
                Line::styled("▢ Ship", Style::default().fg(Color::Green).bold()),
                Line::styled("? Unknown", Style::default().fg(Color::Yellow).bold()),
                Line::styled("X Hostile", Style::default().fg(Color::Red).bold()),
                Line::styled("+ Generic", Style::default().fg(Color::White).bold()),
                Line::styled("* Weather", Style::default().fg(Color::Magenta).bold()),
            ]);

            let legend = Paragraph::new(legend_text)
                .block(Block::default().borders(Borders::ALL).title("Legend"));
            f.render_widget(legend, control_chunks[2]);

            // Controls panel
            let controls = Paragraph::new("Q - Quit\nSPACE - Reset\nR - Range\nF - Filter\n")
                .block(Block::default().borders(Borders::ALL).title("Controls"));
            f.render_widget(controls, control_chunks[3]);
        })?;

        Ok(())
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        let _ = self.exit();
    }
}
