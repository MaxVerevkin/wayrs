//! Parser for wayland protocol xml files

mod parser;
mod types;

pub use parser::Error;
pub use types::*;

pub fn parse_protocol(text: &str) -> Result<Protocol<'_>, Error> {
    parser::Parser::new(text).get_grotocol()
}
