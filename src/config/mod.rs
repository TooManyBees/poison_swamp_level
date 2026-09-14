mod config;
mod logging;
mod parse;
mod parse_error;
mod time;

pub use config::{Config, ServerMode};
pub use logging::init_logger;
pub use parse::load_config;
pub use parse_error::ParseError;

pub fn config_path() -> Result<String, ()> {
    let mut args = std::env::args().skip(1);
    while args.len() > 0 {
        match args.next().unwrap().to_lowercase().as_str() {
            "-c" | "--config" => match args.next() {
                Some(c) => return Ok(c),
                None => return Err(()),
            },
            _arg => {}
        }
    }
    Ok("./config.kdl".into())
}
