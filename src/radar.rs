use rand::Rng;
use ratatui::buffer::Buffer;
use ratatui::style::Modifier;
use ratatui::widgets::Widget;
use ratatui::widgets::canvas::Line;
use ratatui::{
    layout::Rect,
    style::Color,
    text,
    widgets::canvas::{Canvas, Circle},
};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct Contact {
    pub id: u32,
    pub angle: f64,
    pub distance: f64,
    pub last_hit_time: Instant,
    pub visibility: f64,
    pub object_type: ObjectType,
}

#[derive(Debug, Clone)]
pub struct WorldObjects {
    pub id: u32,
    pub angle: f64,
    pub distance: f64,
    pub object_type: ObjectType,
    pub velocity: (f64, f64),
}

#[derive(Debug, Clone)]
pub enum ObjectType {
    AirCraft,
    Ship,
    Unknown,
    Hostile,
    Generic,
    Weather,
}

pub struct RadarWidget {
    pub sweep_angle: f64,
    pub detected_contacts: Vec<Contact>,
    pub world_objects: Vec<WorldObjects>,
    pub max_range: f64,
    center_x: f64,
    center_y: f64,
    pub fade_duration: f64,
}

impl RadarWidget {
    pub const DEGREES_PER_SECOND: f64 = 48.0;
    pub fn new(max_range: f64, fade_duration: f64) -> Self {
        Self {
            sweep_angle: 0.0,
            detected_contacts: Vec::new(),
            world_objects: Vec::new(),
            max_range,
            center_x: 0.0,
            center_y: 0.0,
            fade_duration,
        }
    }

    pub fn update_sweep(&mut self, delta_time: f64) {
        let old_angle = self.sweep_angle;
        self.sweep_angle += delta_time * RadarWidget::DEGREES_PER_SECOND;
        if self.sweep_angle >= 360.0 {
            self.sweep_angle -= 360.0;
        }

        self.update_target_visibility();

        // Check for sweep hits
        self.check_sweep_hits(old_angle);
    }

    fn update_target_visibility(&mut self) {
        let now = Instant::now();

        // Remove contacts that are too old haven't been hit in 2 full sweeps
        let max_age = self.fade_duration * 2.0;
        self.detected_contacts
            .retain(|contact| now.duration_since(contact.last_hit_time).as_secs_f64() < max_age);

        // Update visibility for remaining contacts
        for target in &mut self.detected_contacts {
            let time_since_hit = now.duration_since(target.last_hit_time).as_secs_f64();
            if time_since_hit < self.fade_duration {
                target.visibility = (1.0 - (time_since_hit / self.fade_duration)).max(0.0);
            } else {
                target.visibility = 0.0;
            }
        }
    }
    fn check_sweep_hits(&mut self, old_angle: f64) {
        let now = Instant::now();

        for world_obj in &self.world_objects {
            if self.sweep_crossed_target(old_angle, self.sweep_angle, world_obj.angle) {
                if let Some(contact) = self
                    .detected_contacts
                    .iter_mut()
                    .find(|c| c.id == world_obj.id)
                {
                    // Update existing contact with new position
                    contact.angle = world_obj.angle;
                    contact.distance = world_obj.distance;
                    contact.last_hit_time = now;
                    contact.visibility = 1.0;
                } else {
                    // Create new contact
                    self.detected_contacts.push(Contact {
                        id: world_obj.id,
                        angle: world_obj.angle,
                        distance: world_obj.distance,
                        last_hit_time: now,
                        visibility: 1.0,
                        object_type: world_obj.object_type.clone(),
                    });
                }
                // print!("\x07"); Bell audio
            }
        }
    }

    fn sweep_crossed_target(&self, old_angle: f64, new_angle: f64, target_angle: f64) -> bool {
        let sweep_width = 3.0; // Degrees of sweep beam width

        // Handle sweep crossing 0/360 boundary
        if new_angle < old_angle {
            // Sweep crossed 0 degrees
            if target_angle >= old_angle || target_angle <= new_angle {
                let angle_diff = if target_angle >= old_angle {
                    new_angle + (360.0 - old_angle)
                } else {
                    new_angle - 0.0
                };
                return angle_diff <= sweep_width;
            }
        } else {
            // Normal case
            if target_angle >= old_angle && target_angle <= new_angle {
                return (target_angle - old_angle) <= sweep_width;
            }
        }
        false
    }
}

impl Widget for &RadarWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let canvas = Canvas::default()
            .x_bounds([-self.max_range, self.max_range])
            .y_bounds([-self.max_range, self.max_range])
            .paint(|ctx| {
                // Draw range rings
                for i in 1..=4 {
                    let radius = (i as f64) * (self.max_range / 4.0);
                    ctx.draw(&Circle {
                        x: self.center_x,
                        y: self.center_y,
                        radius,
                        color: Color::Green,
                    });
                }

                // Draw bearing lines
                for angle in (0..360).step_by(30) {
                    let rad = (angle as f64).to_radians();

                    let start_distance = self.max_range * 0.1;
                    let start_x = self.center_x + start_distance * rad.cos();
                    let start_y = self.center_y + start_distance * rad.sin();

                    let end_x = self.center_x + self.max_range * rad.cos();
                    let end_y = self.center_y + self.max_range * rad.sin();

                    ctx.draw(&Line {
                        x1: start_x,
                        y1: start_y,
                        x2: end_x,
                        y2: end_y,
                        color: Color::DarkGray,
                    });
                }

                // Danger close range
                ctx.draw(&Circle {
                    x: self.center_x,
                    y: self.center_y,
                    radius: self.max_range * 0.1,
                    color: Color::Blue,
                });

                // US / center
                ctx.draw(&Circle {
                    x: self.center_x,
                    y: self.center_y,
                    radius: 2.0,
                    color: Color::Blue,
                });

                // Draw sweep line
                let sweep_rad = self.sweep_angle.to_radians();
                let sweep_end_x = self.center_x + self.max_range * sweep_rad.cos();
                let sweep_end_y = self.center_y + self.max_range * sweep_rad.sin();

                ctx.draw(&Line {
                    x1: self.center_x,
                    y1: self.center_y,
                    x2: sweep_end_x,
                    y2: sweep_end_y,
                    color: Color::Yellow,
                });

                // drawing detected contacts
                for contact in &self.detected_contacts {
                    if contact.visibility > 0.0 {
                        let symbol = contact.object_type.symbol();
                        let color = contact.object_type.color();

                        let rad = contact.angle.to_radians();
                        let x = self.center_x + contact.distance * rad.cos();
                        let y = self.center_y + contact.distance * rad.sin();

                        let intensity = (255.0 * contact.visibility) as u8;
                        let faded_color = match color {
                            Color::Red => Color::Rgb(intensity, 0, 0),
                            Color::Green => Color::Rgb(0, intensity, 0),
                            Color::Blue => Color::Rgb(0, 0, intensity),
                            Color::Cyan => Color::Rgb(0, intensity, intensity),
                            Color::Yellow => Color::Rgb(intensity, intensity, 0),
                            Color::Magenta => Color::Rgb(intensity, 0, intensity),
                            Color::White => Color::Rgb(intensity, intensity, intensity),
                            _ => Color::Rgb(intensity, intensity, 0), //yellow
                        };

                        let line = text::Line::from(format!("{symbol}"))
                            .style((faded_color, Modifier::BOLD));
                        ctx.print(x, y, line);
                    }
                }
            });
        canvas.render(area, buf);
    }
}

impl RadarWidget {
    pub fn update_world_objects(&mut self, delta_time: f64) {
        for obj in &mut self.world_objects {
            // Update position based on velocity
            obj.angle += obj.velocity.0 * delta_time;
            obj.distance += obj.velocity.1 * delta_time;

            // Wrap angle around
            if obj.angle >= 360.0 {
                obj.angle -= 360.0;
            } else if obj.angle < 0.0 {
                obj.angle += 360.0;
            }
        }

        // Remove objects that moved too far away
        self.world_objects
            .retain(|obj| obj.distance > 0.0 && obj.distance <= self.max_range);
    }
    pub fn spawn_aircraft(&mut self, id: u32) {
        let mut rng = rand::rng();

        // Spawn at edge, flying across
        let start_angle = rng.random_range(0.0..360.0);
        let target_angle = rng.random_range(0.0..360.0);

        // Calculate angular velocity to fly toward target
        let mut angle_diff = target_angle - start_angle;
        if angle_diff > 180.0 {
            angle_diff -= 360.0;
        }
        if angle_diff < -180.0 {
            angle_diff += 360.0;
        }

        let angular_velocity = angle_diff / 120.0;
        let radial_velocity = rng.random_range(-2.0..2.0);

        self.world_objects.push(WorldObjects {
            id,
            angle: start_angle,
            distance: self.max_range * 0.9,
            object_type: ObjectType::AirCraft,
            velocity: (angular_velocity, radial_velocity),
        });
    }

    pub fn spawn_ship(&mut self, id: u32) {
        let mut rng = rand::rng();

        self.world_objects.push(WorldObjects {
            id,
            angle: rng.random_range(0.0..360.0),
            distance: rng.random_range(20.0..80.0),
            object_type: ObjectType::Ship,
            velocity: (rng.random_range(-2.0..2.0), rng.random_range(-1.0..1.0)),
        });
    }
}

impl RadarWidget {
    // Add the missing spawn methods
    pub fn spawn_unknown(&mut self, id: u32) {
        let mut rng = rand::rng();

        self.world_objects.push(WorldObjects {
            id,
            angle: rng.random_range(0.0..360.0),
            distance: rng.random_range(30.0..self.max_range * 0.8),
            object_type: ObjectType::Unknown,
            velocity: (rng.random_range(-1.0..1.0), rng.random_range(-2.0..2.0)),
        });
    }

    pub fn spawn_hostile(&mut self, id: u32) {
        let mut rng = rand::rng();

        // Hostiles move faster and more aggressively
        self.world_objects.push(WorldObjects {
            id,
            angle: rng.random_range(0.0..360.0),
            distance: rng.random_range(40.0..self.max_range * 0.7),
            object_type: ObjectType::Hostile,
            velocity: (rng.random_range(-8.0..8.0), rng.random_range(-8.0..8.0)),
        });
    }

    pub fn spawn_generic(&mut self, id: u32) {
        let mut rng = rand::rng();

        self.world_objects.push(WorldObjects {
            id,
            angle: rng.random_range(0.0..360.0),
            distance: rng.random_range(15.0..self.max_range * 0.9),
            object_type: ObjectType::Generic,
            velocity: (rng.random_range(-3.0..3.0), rng.random_range(-3.0..3.0)),
        });
    }

    pub fn spawn_weather(&mut self, id: u32) {
        let mut rng = rand::rng();

        // Weather moves slowly and changes size/intensity
        self.world_objects.push(WorldObjects {
            id,
            angle: rng.random_range(0.0..360.0),
            distance: rng.random_range(10.0..self.max_range * 0.6),
            object_type: ObjectType::Weather,
            velocity: (rng.random_range(-0.1..0.1), rng.random_range(-0.2..0.2)),
        });
    }

    pub fn spawn_random_object(&mut self, id: u32) {
        let mut rng = rand::rng();

        match rng.random_range(0..6) {
            0 => self.spawn_aircraft(id),
            1 => self.spawn_ship(id),
            2 => self.spawn_unknown(id),
            3 => self.spawn_hostile(id),
            4 => self.spawn_generic(id),
            5 => self.spawn_weather(id),
            _ => self.spawn_generic(id),
        }
    }
}

impl ObjectType {
    pub fn symbol(&self) -> char {
        match self {
            ObjectType::AirCraft => '^',
            ObjectType::Ship => '▢',
            ObjectType::Unknown => '?',
            ObjectType::Hostile => 'X',
            ObjectType::Generic => '+',
            ObjectType::Weather => '*',
        }
    }

    pub fn color(&self) -> Color {
        match self {
            ObjectType::AirCraft => Color::Cyan,
            ObjectType::Ship => Color::Green,
            ObjectType::Unknown => Color::Yellow,
            ObjectType::Hostile => Color::Red,
            ObjectType::Generic => Color::White,
            ObjectType::Weather => Color::Magenta,
        }
    }
}
