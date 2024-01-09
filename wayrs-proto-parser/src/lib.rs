//! Parser for wayland protocol xml files

mod parser;
mod types;

pub use types::*;

pub fn parse_protocol(text: &str) -> Protocol<'_> {
    parser::Parser::new(text).get_grotocol()
}
