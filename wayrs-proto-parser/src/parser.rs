use crate::types::*;
use quick_xml::events::{BytesStart, Event as XmlEvent};

pub struct Parser<'a> {
    reader: quick_xml::Reader<&'a [u8]>,
}

impl<'a> Parser<'a> {
    pub fn new(str: &'a str) -> Self {
        let mut reader = quick_xml::Reader::from_str(str);
        reader.trim_text(true);
        Self { reader }
    }

    pub fn get_grotocol(mut self) -> Protocol<'a> {
        loop {
            match self.reader.read_event().unwrap() {
                XmlEvent::Eof => panic!("unexpeced EOF"),
                XmlEvent::Start(start) => match start.name().as_ref() {
                    b"protocol" => return self.parse_protocol(start),
                    x => {
                        let tag_name = std::str::from_utf8(x).unwrap();
                        panic!("unexpeced tag: {tag_name}");
                    }
                },
                _ => (),
            }
        }
    }

    fn parse_protocol(&mut self, tag: BytesStart<'a>) -> Protocol<'a> {
        let mut protocol = Protocol {
            name: tag
                .try_get_attribute("name")
                .unwrap()
                .unwrap()
                .unescape_value()
                .unwrap()
                .into_owned(),
            description: None,
            interfaces: Vec::new(),
        };

        loop {
            match self.reader.read_event().unwrap() {
                XmlEvent::Eof => panic!("unexpeced EOF"),
                XmlEvent::Start(start) => match start.name().as_ref() {
                    b"protocol" => panic!("nested protocol"),
                    b"description" => protocol.description = Some(self.parse_description(start)),
                    b"interface" => protocol.interfaces.push(self.parse_interface(start)),
                    b"copyright" => {
                        // TODO?
                    }
                    x => {
                        let tag_name = std::str::from_utf8(x).unwrap();
                        panic!("unexpeced tag: {tag_name}");
                    }
                },
                XmlEvent::End(end) if end.name() == tag.name() => break,
                _ => (),
            }
        }

        protocol
    }

    fn parse_interface(&mut self, tag: BytesStart<'a>) -> Interface<'a> {
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
                XmlEvent::Eof => panic!("unexpeced EOF"),
                XmlEvent::Start(start) => match start.name().as_ref() {
                    b"interface" => panic!("nested interface"),
                    b"description" => interface.description = Some(self.parse_description(start)),
                    b"request" => interface.requests.push(self.parse_message(start)),
                    b"event" => interface.events.push(self.parse_message(start)),
                    b"enum" => interface.enums.push(self.parse_enum(start)),
                    x => {
                        let tag_name = std::str::from_utf8(x).unwrap();
                        panic!("unexpeced tag: {tag_name}");
                    }
                },
                XmlEvent::End(end) if end.name().as_ref() == b"interface" => break,
                _ => (),
            }
        }

        interface
    }

    fn parse_message(&mut self, tag: BytesStart<'a>) -> Message<'a> {
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
                XmlEvent::Eof => panic!("unexpeced EOF"),
                XmlEvent::Start(start) => match start.name().as_ref() {
                    b"description" => message.description = Some(self.parse_description(start)),
                    other => {
                        let tag_name = std::str::from_utf8(other).unwrap();
                        panic!("unhandled tag: {tag_name}");
                    }
                },
                XmlEvent::Empty(empty) => match empty.name().as_ref() {
                    b"arg" => message.args.push(Self::parse_arg(empty)),
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
                    other => {
                        let tag_name = std::str::from_utf8(other).unwrap();
                        panic!("unhandled tag: {tag_name}");
                    }
                },
                XmlEvent::End(end) if end.name() == tag.name() => break,
                _ => (),
            }
        }

        message
    }

    fn parse_enum(&mut self, tag: BytesStart<'a>) -> Enum<'a> {
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
                XmlEvent::Eof => panic!("unexpeced EOF"),
                XmlEvent::Empty(empty) => match empty.name().as_ref() {
                    b"entry" => en.items.push(self.parse_enum_item(empty, false)),
                    other => {
                        let tag_name = std::str::from_utf8(other).unwrap();
                        panic!("unhandled tag: {tag_name}");
                    }
                },
                XmlEvent::Start(start) => match start.name().as_ref() {
                    b"description" => en.description = Some(self.parse_description(start)),
                    b"entry" => en.items.push(self.parse_enum_item(start, true)),
                    other => {
                        let tag_name = std::str::from_utf8(other).unwrap();
                        panic!("unhandled tag: {tag_name}");
                    }
                },
                XmlEvent::End(end) if end.name() == tag.name() => break,
                _ => (),
            }
        }

        en
    }

    fn parse_description(&mut self, tag: BytesStart<'a>) -> Description<'a> {
        let mut description = Description {
            summary: tag
                .try_get_attribute("summary")
                .unwrap()
                .map(|attr| attr.unescape_value().unwrap().into_owned()),
            text: None,
        };

        loop {
            match self.reader.read_event().unwrap() {
                XmlEvent::Eof => panic!("unexpeced EOF"),
                XmlEvent::Text(text) => description.text = Some(text.unescape().unwrap()),
                XmlEvent::End(end) if end.name() == tag.name() => break,
                _ => (),
            }
        }

        description
    }

    fn parse_arg(arg: BytesStart<'a>) -> Argument {
        let mut name = None;
        let mut arg_type = None;
        let mut allow_null = false;
        let mut enum_type = None;
        let mut interface = None;
        let mut summary = None;

        for attr in arg.attributes().with_checks(false) {
            let attr = attr.unwrap();
            match attr.key.as_ref() {
                b"name" => name = Some(attr.unescape_value().unwrap().into_owned()),
                b"type" => arg_type = Some(attr.unescape_value().unwrap().into_owned()),
                b"enum" => enum_type = Some(attr.unescape_value().unwrap().into_owned()),
                b"interface" => interface = Some(attr.unescape_value().unwrap().into_owned()),
                b"summary" => summary = Some(attr.unescape_value().unwrap().into_owned()),
                b"allow-null" => allow_null = attr.unescape_value().unwrap() == "true",
                _ => (),
            }
        }

        Argument {
            name: name.unwrap(),
            arg_type: arg_type.unwrap(),
            allow_null,
            enum_type,
            interface,
            summary,
        }
    }

    fn parse_enum_item(&mut self, arg: BytesStart<'a>, non_empty_tag: bool) -> EnumItem {
        let mut name = None;
        let mut value = None;
        let mut summary = None;
        let mut since = 1;

        for attr in arg.attributes().with_checks(false) {
            let attr = attr.unwrap();
            match attr.key.as_ref() {
                b"name" => name = Some(attr.unescape_value().unwrap().into_owned()),
                b"value" => value = Some(attr.unescape_value().unwrap().into_owned()),
                b"since" => since = attr.unescape_value().unwrap().parse().unwrap(),
                b"summary" => summary = Some(attr.unescape_value().unwrap().into_owned()),
                _ => (),
            }
        }

        if non_empty_tag {
            loop {
                match self.reader.read_event().unwrap() {
                    XmlEvent::Eof => panic!("unexpeced EOF"),
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

        EnumItem {
            name: name.unwrap(),
            value: value.unwrap(),
            since,
            description: summary.map(|summary| Description {
                summary: Some(summary),
                text: None,
            }),
        }
    }
}
