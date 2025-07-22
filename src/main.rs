use tui_radar_sim_core::tui::{MyResult, Tui};

fn main() -> MyResult<()> {
    let mut tui = Tui::new(30.0, 15.0)?;
    tui.run()?;
    Ok(())
}
