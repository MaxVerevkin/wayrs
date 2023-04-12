use std::borrow::Cow;

#[derive(Debug)]
pub struct Protocol<'a> {
    pub name: String,
    pub description: Option<Description<'a>>,
    pub interfaces: Vec<Interface<'a>>,
}

#[derive(Debug)]
pub struct Interface<'a> {
    pub name: String,
    pub version: u32,
    pub description: Option<Description<'a>>,
    pub requests: Vec<Message<'a>>,
    pub events: Vec<Message<'a>>,
    pub enums: Vec<Enum<'a>>,
}

#[derive(Debug)]
pub struct Message<'a> {
    pub name: String,
    pub kind: Option<String>,
    pub since: u32,
    pub description: Option<Description<'a>>,
    pub args: Vec<Argument>,
}

#[derive(Debug)]
pub struct Enum<'a> {
    pub name: String,
    pub is_bitfield: bool,
    pub description: Option<Description<'a>>,
    pub items: Vec<EnumItem>,
}

#[derive(Debug)]
pub struct Description<'a> {
    pub summary: Option<String>,
    pub text: Option<Cow<'a, str>>,
}

#[derive(Debug)]
pub struct Argument {
    pub name: String,
    pub arg_type: String,
    pub allow_null: bool,
    pub enum_type: Option<String>,
    pub interface: Option<String>,
    pub summary: Option<String>,
}

#[derive(Debug)]
pub struct EnumItem {
    pub name: String,
    pub value: u32,
    pub summary: Option<String>,
}
