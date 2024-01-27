use std::borrow::Cow;

#[derive(Debug, Clone)]
pub struct Protocol<'a> {
    pub name: String,
    pub description: Option<Description<'a>>,
    pub interfaces: Vec<Interface<'a>>,
}

#[derive(Debug, Clone)]
pub struct Interface<'a> {
    pub name: String,
    pub version: u32,
    pub description: Option<Description<'a>>,
    pub requests: Vec<Message<'a>>,
    pub events: Vec<Message<'a>>,
    pub enums: Vec<Enum<'a>>,
}

#[derive(Debug, Clone)]
pub struct Message<'a> {
    pub name: String,
    pub kind: Option<String>,
    pub since: u32,
    pub description: Option<Description<'a>>,
    pub args: Vec<Argument>,
}

#[derive(Debug, Clone)]
pub struct Enum<'a> {
    pub name: String,
    pub is_bitfield: bool,
    pub description: Option<Description<'a>>,
    pub items: Vec<EnumItem>,
}

#[derive(Debug, Clone)]
pub struct Description<'a> {
    pub summary: Option<String>,
    pub text: Option<Cow<'a, str>>,
}

#[derive(Debug, Clone)]
pub struct Argument {
    pub name: String,
    pub arg_type: ArgType,
    pub summary: Option<String>,
}

/// The types of wayland message argumests
///
/// Spec: <https://wayland.freedesktop.org/docs/html/ch04.html>
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgType {
    /// 32-bit signed integer.
    Int,
    /// 32-bit unsigend integer.
    Uint,
    /// 32-bit integer referencing a value of a given enum.
    Enum(String),
    /// Sigend 24.8 decimal number.
    Fixed,
    /// Length-prefixed null-terimnated string.
    String { allow_null: bool },
    /// 32-bit unsigned integer referring to an object.
    Object {
        allow_null: bool,
        iface: Option<String>,
    },
    /// 32-bit unsigned integer informing about object creation.
    NewId { iface: Option<String> },
    /// Length-prefixed array.
    Array,
    /// A file descriptor in the ancillary data.
    Fd,
}

#[derive(Debug, Clone)]
pub struct EnumItem {
    pub name: String,
    pub value: u32,
    pub since: u32,
    pub description: Option<Description<'static>>,
}
