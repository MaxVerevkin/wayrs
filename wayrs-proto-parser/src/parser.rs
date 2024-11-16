use std::fmt;
use std::str;

use quick_xml::events::{BytesStart, Event as XmlEvent};

use crate::types::*;

pub struct Parser<'a> {
    reader: quick_xml::Reader<&'a [u8]>,
}

#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    UnexpectedTag(String),
    UnexpectedArgType(String),
    UnexpectedEof,
    ProtocolWithoutName,
    XmlError(String),
    NonUtf8Data(str::Utf8Error),
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedTag(tag) => write!(f, "unexpected tag: {tag}"),
            Self::UnexpectedArgType(ty) => write!(f, "unexpected argument type: {ty}"),
            Self::UnexpectedEof => f.write_str("unexpeced end of file"),
            Self::ProtocolWithoutName => f.write_str("protocol does not have a name"),
            Self::XmlError(error) => write!(f, "xml parsing error: {error}"),
            Self::NonUtf8Data(utf8_error) => utf8_error.fmt(f),
        }
    }
}

impl From<quick_xml::Error> for Error {
    fn from(value: quick_xml::Error) -> Self {
        Self::XmlError(value.to_string())
    }
}

impl From<str::Utf8Error> for Error {
    fn from(value: str::Utf8Error) -> Self {
        Self::NonUtf8Data(value)
    }
}

impl<'a> Parser<'a> {
    pub fn new(str: &'a str) -> Self {
        let mut reader = quick_xml::Reader::from_str(str);
        reader.config_mut().trim_text(true);
        Self { reader }
    }

    pub fn get_grotocol(mut self) -> Result<Protocol<'a>, Error> {
        loop {
            match self.reader.read_event()? {
                XmlEvent::Eof => return Err(Error::UnexpectedEof),
                XmlEvent::Start(start) => match start.name().as_ref() {
                    b"protocol" => return self.parse_protocol(start),
                    other => return Err(Error::UnexpectedTag(str::from_utf8(other)?.into())),
                },
                _ => (),
            }
        }
    }

    fn parse_protocol(&mut self, tag: BytesStart<'a>) -> Result<Protocol<'a>, Error> {
        let mut protocol = Protocol {
            name: tag
                .try_get_attribute("name")
                .ok()
                .flatten()
                .ok_or(Error::ProtocolWithoutName)?
                .unescape_value()?
                .into_owned(),
            description: None,
            interfaces: Vec::new(),
        };

        loop {
            match self.reader.read_event()? {
                XmlEvent::Eof => return Err(Error::UnexpectedEof),
                XmlEvent::Start(start) => match start.name().as_ref() {
                    b"description" => protocol.description = Some(self.parse_description(start)?),
                    b"interface" => protocol.interfaces.push(self.parse_interface(start)?),
                    b"copyright" => {
                        // TODO?
                    }
                    other => return Err(Error::UnexpectedTag(str::from_utf8(other)?.into())),
                },
                XmlEvent::End(end) if end.name() == tag.name() => break,
                _ => (),
            }
        }

        Ok(protocol)
    }

    fn parse_interface(&mut self, tag: BytesStart<'a>) -> Result<Interface<'a>, Error> {
        let mut interface = Interface {
            name: tag
                .try_get_attribute("name")
                .unwrap()
                .unwrap()
                .unescape_value()
                .unwrap()
                .into_owned(),
            version: tag
                .try_get_attribute("version")
                .unwrap()
                .unwrap()
                .unescape_value()
                .unwrap()
                .parse()
                .unwrap(),
            description: None,
            requests: Vec::new(),
            events: Vec::new(),
            enums: Vec::new(),
        };

        loop {
            match self.reader.read_event().unwrap() {
                XmlEvent::Eof => return Err(Error::UnexpectedEof),
                XmlEvent::Start(start) => match start.name().as_ref() {
                    b"description" => interface.description = Some(self.parse_description(start)?),
                    b"request" => interface.requests.push(self.parse_message(start)?),
                    b"event" => interface.events.push(self.parse_message(start)?),
                    b"enum" => interface.enums.push(self.parse_enum(start)?),
                    other => return Err(Error::UnexpectedTag(str::from_utf8(other)?.into())),
                },
                XmlEvent::End(end) if end.name().as_ref() == b"interface" => break,
                _ => (),
            }
        }

        Ok(interface)
    }

    fn parse_message(&mut self, tag: BytesStart<'a>) -> Result<Message<'a>, Error> {
        let mut name = None;
        let mut kind = None;
        let mut since = 1;

        for attr in tag.attributes() {
            let attr = attr.unwrap();
            match attr.key.as_ref() {
                b"name" => name = Some(attr.unescape_value().unwrap().into_owned()),
                b"type" => kind = Some(attr.unescape_value().unwrap().into_owned()),
                b"since" => since = attr.unescape_value().unwrap().parse().unwrap(),
                _ => (),
            }
        }

        let mut message = Message {
            name: name.unwrap(),
            kind,
            since,
            description: None,
            args: Vec::new(),
        };

        loop {
            match self.reader.read_event().unwrap() {
                XmlEvent::Eof => return Err(Error::UnexpectedEof),
                XmlEvent::Start(start) => match start.name().as_ref() {
                    b"description" => message.description = Some(self.parse_description(start)?),
                    other => return Err(Error::UnexpectedTag(str::from_utf8(other)?.into())),
                },
                XmlEvent::Empty(empty) => match empty.name().as_ref() {
                    b"arg" => message.args.push(Self::parse_arg(empty)?),
                    b"description" => {
                        let summary = empty
                            .try_get_attribute("summary")
                            .unwrap()
                            .map(|attr| attr.unescape_value().unwrap().into_owned());
                        message.description = Some(Description {
                            summary,
                            text: None,
                        });
                    }
                    other => return Err(Error::UnexpectedTag(str::from_utf8(other)?.into())),
                },
                XmlEvent::End(end) if end.name() == tag.name() => break,
                _ => (),
            }
        }

        Ok(message)
    }

    fn parse_enum(&mut self, tag: BytesStart<'a>) -> Result<Enum<'a>, Error> {
        let mut en = Enum {
            name: tag
                .try_get_attribute("name")
                .unwrap()
                .unwrap()
                .unescape_value()
                .unwrap()
                .into_owned(),
            is_bitfield: tag
                .try_get_attribute("bitfield")
                .unwrap()
                .map_or(false, |attr| attr.unescape_value().unwrap() == "true"),
            description: None,
            items: Vec::new(),
        };

        loop {
            match self.reader.read_event().unwrap() {
                XmlEvent::Eof => return Err(Error::UnexpectedEof),
                XmlEvent::Empty(empty) => match empty.name().as_ref() {
                    b"entry" => en.items.push(self.parse_enum_item(empty, false)?),
                    other => return Err(Error::UnexpectedTag(str::from_utf8(other)?.into())),
                },
                XmlEvent::Start(start) => match start.name().as_ref() {
                    b"description" => en.description = Some(self.parse_description(start)?),
                    b"entry" => en.items.push(self.parse_enum_item(start, true)?),
                    other => return Err(Error::UnexpectedTag(str::from_utf8(other)?.into())),
                },
                XmlEvent::End(end) if end.name() == tag.name() => break,
                _ => (),
            }
        }

        Ok(en)
    }

    fn parse_description(&mut self, tag: BytesStart<'a>) -> Result<Description<'a>, Error> {
        let mut description = Description {
            summary: tag
                .try_get_attribute("summary")
                .unwrap()
                .map(|attr| attr.unescape_value().unwrap().into_owned()),
            text: None,
        };

        loop {
            match self.reader.read_event().unwrap() {
                XmlEvent::Eof => return Err(Error::UnexpectedEof),
                XmlEvent::Text(text) => description.text = Some(text.unescape().unwrap()),
                XmlEvent::End(end) if end.name() == tag.name() => break,
                _ => (),
            }
        }

        Ok(description)
    }

    fn parse_arg(arg: BytesStart<'a>) -> Result<Argument, Error> {
        let mut name = None;
        let mut arg_type = None;
        let mut allow_null = false;
        let mut enum_ty = None;
        let mut iface = None;
        let mut summary = None;

        for attr in arg.attributes().with_checks(false) {
            let attr = attr.unwrap();
            match attr.key.as_ref() {
                b"name" => name = Some(attr.unescape_value()?.into_owned()),
                b"type" => arg_type = Some(attr.unescape_value()?.into_owned()),
                b"enum" => enum_ty = Some(attr.unescape_value()?.into_owned()),
                b"interface" => iface = Some(attr.unescape_value()?.into_owned()),
                b"summary" => summary = Some(attr.unescape_value()?.into_owned()),
                b"allow-null" => allow_null = attr.unescape_value()? == "true",
                _ => (),
            }
        }

        Ok(Argument {
            name: name.unwrap(),
            arg_type: match arg_type.unwrap().as_str() {
                "int" | "uint" if enum_ty.is_some() => ArgType::Enum(enum_ty.unwrap()),
                "int" => ArgType::Int,
                "uint" => ArgType::Uint,
                "fixed" => ArgType::Fixed,
                "string" => ArgType::String { allow_null },
                "object" => ArgType::Object { allow_null, iface },
                "new_id" => ArgType::NewId { iface },
                "array" => ArgType::Array,
                "fd" => ArgType::Fd,
                other => return Err(Error::UnexpectedArgType(other.into())),
            },
            summary,
        })
    }

    fn parse_enum_item(
        &mut self,
        arg: BytesStart<'a>,
        non_empty_tag: bool,
    ) -> Result<EnumItem, Error> {
        let mut name = None;
        let mut value = None;
        let mut summary = None;
        let mut since = 1;

        for attr in arg.attributes().with_checks(false) {
            let attr = attr.unwrap();
            match attr.key.as_ref() {
                b"name" => name = Some(attr.unescape_value()?.into_owned()),
                b"value" => value = Some(attr.unescape_value()?.into_owned()),
                b"since" => since = attr.unescape_value()?.parse().unwrap(),
                b"summary" => summary = Some(attr.unescape_value()?.into_owned()),
                _ => (),
            }
        }

        if non_empty_tag {
            loop {
                match self.reader.read_event().unwrap() {
                    XmlEvent::Eof => return Err(Error::UnexpectedEof),
                    // TODO
                    // XmlEvent::Text(text) => description.text = Some(text.unescape().unwrap()),
                    XmlEvent::End(end) if end.name() == arg.name() => break,
                    _ => (),
                }
            }
        }

        let value = value.map(|v| {
            if let Some(v) = v.strip_prefix("0x") {
                u32::from_str_radix(v, 16).unwrap()
            } else {
                v.parse().unwrap()
            }
        });

        Ok(EnumItem {
            name: name.unwrap(),
            value: value.unwrap(),
            since,
            description: summary.map(|summary| Description {
                summary: Some(summary),
                text: None,
            }),
        })
    }
}
