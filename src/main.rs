mod window;
mod config;

// Import the applet model (Window)
use crate::window::Window;

// The main function returns a cosmic::iced::Result that is returned from
// the run function that's part of the applet module.
fn main() -> cosmic::iced::Result {
    tracing_subscriber::fmt::init();
    let _ = tracing_log::LogTracer::init();
    
    cosmic::applet::run::<Window>(())?;

    Ok(())
}