mod parser;
mod types;

use std::path::PathBuf;

use proc_macro2::{Span, TokenStream};
use quote::quote;

use crate::parser::Parser;
use crate::types::*;
use convert_case::{Case, Casing};

#[proc_macro]
pub fn generate(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let path = syn::parse_macro_input!(input as syn::LitStr).value();
    let path = match std::env::var_os("CARGO_MANIFEST_DIR") {
        Some(manifest) => {
            let mut full = PathBuf::from(manifest);
            full.push(path);
            full
        }
        None => PathBuf::from(path),
    };

    let file = std::fs::read_to_string(path).expect("could not read the file");
    let parser = Parser::new(&file);
    let protocol = parser.get_grotocol();

    let modules = protocol
        .interfaces
        .iter()
        .filter(|iface| iface.name != "wl_display")
        .map(gen_module_for_interface);
    let expanded = quote! { #(#modules)* };

    // let mut rustfmt = std::process::Command::new("rustfmt")
    //     .stdin(std::process::Stdio::piped())
    //     .stdout(std::process::Stdio::inherit())
    //     .spawn()
    //     .unwrap();
    // let mut rustfmt_in = rustfmt.stdin.take().unwrap();
    // use std::io::Write;
    // write!(rustfmt_in, "{}", expanded).unwrap();
    // drop(rustfmt_in);
    // let formated = rustfmt.wait_with_output().unwrap().stdout;
    // let formated = String::from_utf8(formated).unwrap();
    // eprintln!("  {formated}");

    expanded.into()
}

fn gen_module_for_interface(iface: &Interface) -> TokenStream {
    let doc = gen_doc(&iface.description);
    let name = syn::Ident::new(&iface.name, Span::call_site());
    let interface = gen_interface(iface);
    quote! {
        #doc
        pub mod #name {
            #interface
        }
    }
}

fn make_ident(name: impl AsRef<str>) -> syn::Ident {
    syn::Ident::new_raw(name.as_ref(), Span::call_site())
}

fn make_pascal_case_ident(name: impl AsRef<str>) -> syn::Ident {
    let name = name.as_ref();
    if name.chars().next().unwrap().is_ascii_digit() {
        syn::Ident::new_raw(
            &format!("_{}", name.to_case(Case::Pascal)),
            Span::call_site(),
        )
    } else {
        syn::Ident::new_raw(&name.to_case(Case::Pascal), Span::call_site())
    }
}

fn make_proxy_path(iface: impl AsRef<str>) -> TokenStream {
    let iface_name = syn::Ident::new(iface.as_ref(), Span::call_site());
    let proxy_name = make_pascal_case_ident(iface);
    quote! { super::#iface_name::#proxy_name }
}

fn gen_interface(iface: &Interface) -> TokenStream {
    let static_name = syn::Ident::new(
        &(iface.name.to_uppercase() + "_INTERFACE"),
        Span::call_site(),
    );

    let proxy_name = make_pascal_case_ident(&iface.name);

    let raw_name = &iface.name;
    let version = iface.version;

    let events_desc = iface.events.iter().map(|event| {
        let args = event.args.iter().map(map_arg_to_argtype);
        let name = &event.name;
        let is_destructor = event.kind.as_deref() == Some("destructor");
        quote! {
            super::wayrs_client::interface::MessageDesc {
                name: #name,
                is_destructor: #is_destructor,
                signature: &[ #( super::wayrs_client::wire::ArgType::#args, )* ]
            }
        }
    });
    let requests_desc = iface.requests.iter().map(|request| {
        let args = request.args.iter().map(map_arg_to_argtype);
        let name = &request.name;
        let is_destructor = request.kind.as_deref() == Some("destructor");
        quote! {
            super::wayrs_client::interface::MessageDesc {
                name: #name,
                is_destructor: #is_destructor,
                signature: &[ #( super::wayrs_client::wire::ArgType::#args, )* ]
            }
        }
    });

    let event_args_structs = iface.events.iter().map(|event| {
        let event_name = make_pascal_case_ident(&event.name);
        let struct_name = quote::format_ident!("{event_name}Args");
        let fields = event.args.iter().map(map_arg_to_sturt_field);
        if event.args.len() > 1 {
            quote! {
                #[derive(Debug)]
                pub struct #struct_name { #( pub #fields, )* }
            }
        } else {
            quote!()
        }
    });

    let event_enum_options = iface.events.iter().map(|event| {
        let event_name = make_pascal_case_ident(&event.name);
        let struct_name = quote::format_ident!("{event_name}Args");
        match event.args.as_slice() {
            [] => quote! { #event_name },
            [_, _, ..] => quote! { #event_name(#struct_name) },
            [arg] => {
                let event_ty = map_arg_to_rs(arg);
                quote! { #event_name(#event_ty) }
            }
        }
    });

    let event_decoding = iface.events.iter().enumerate().map(|(opcode, event)| {
        let event_name = make_pascal_case_ident(&event.name);
        let opcode = opcode as u16;
        let struct_name = quote::format_ident!("{event_name}Args");
        let arg_ty = event.args.iter().map(map_arg_to_argval);
        let arg_names = event.args.iter().map(|arg| make_ident(&arg.name));
        let arg_names2 = arg_names.clone();
        match event.args.len() {
            0 => quote! {
                #opcode => {
                    assert!(event.args.is_empty());
                    Ok(Event::#event_name)
                }
            },
            1 => quote! {
                #opcode => {
                    assert_eq!(event.args.len(), 1);
                    let mut args = event.args.into_iter();
                    #( let Some(super::wayrs_client::wire::ArgValue::#arg_ty(arg)) = args.next() else { return Err(super::wayrs_client::proxy::BadMessage) }; )*
                    Ok(Event::#event_name(arg.try_into().unwrap()))
                }
            },
            len => quote! {
                #opcode => {
                    assert_eq!(event.args.len(), #len);
                    let mut args = event.args.into_iter();
                    #( let Some(super::wayrs_client::wire::ArgValue::#arg_ty(#arg_names)) = args.next() else { return Err(super::wayrs_client::proxy::BadMessage) }; )*
                    Ok(Event::#event_name(#struct_name {
                        #( #arg_names2: #arg_names2.try_into().unwrap(), )*
                    }))
                }
            }
        }
    });

    let requests = iface.requests.iter().enumerate().map(|(opcode, request)| {
        let opcode = opcode as u16;
        let new_id_cnt = request
            .args
            .iter()
            .filter(|x| x.arg_type == "new_id")
            .count();
        let new_id_interface = request
            .args
            .iter()
            .find(|x| x.arg_type == "new_id")
            .and_then(|x| x.interface.as_deref());
        let request_name = make_ident(&request.name);
        let fn_args = request.args.iter().map(|arg| {
            let arg_name = make_ident(&arg.name);
            let arg_ty = map_arg_to_rs(arg);
            match (arg.arg_type.as_str(), arg.interface.as_deref()) {
                ("int" | "uint" | "fixed" | "string" | "array" | "fd", None) => quote! { ,#arg_name: #arg_ty },
                ("object", None) => quote! { ,#arg_name: super::wayrs_client::object::Object },
                ("object", Some(i)) => {
                    let proxy_path = make_proxy_path(i);
                    quote! { ,#arg_name: #proxy_path }
                }
                ("new_id", None) => quote! { ,version: u32 },
                ("new_id", Some(_)) => quote! {},
                _ => unreachable!(),
            }
        });
        let msg_args = request.args.iter().map(|arg| {
            let arg_name = make_ident(&arg.name);
            match (arg.arg_type.as_str(), arg.interface.as_deref()) {
                ("int", None) => quote! { super::wayrs_client::wire::ArgValue::Int(#arg_name) },
                ("uint", None) => quote! { super::wayrs_client::wire::ArgValue::Uint(#arg_name.into()) },
                ("fixed", None) => quote! { super::wayrs_client::wire::ArgValue::Fixed(#arg_name) },
                ("object", _) => quote! { super::wayrs_client::wire::ArgValue::Object(super::wayrs_client::proxy::Proxy::id(&#arg_name)) },
                ("new_id", None) => quote! { 
                    super::wayrs_client::wire::ArgValue::String(::std::borrow::Cow::Borrowed(P::interface().name)),
                    super::wayrs_client::wire::ArgValue::Uint(version),
                    super::wayrs_client::wire::ArgValue::NewId(super::wayrs_client::object::Object {
                        id: super::wayrs_client::object::ObjectId::NULL,
                        interface: P::interface(),
                        version,
                    })
                },
                ("new_id", Some(i)) => {
                    let proxy_path = make_proxy_path(i);
                    quote! {
                        super::wayrs_client::wire::ArgValue::NewId(super::wayrs_client::object::Object {
                            id: super::wayrs_client::object::ObjectId::NULL,
                            interface: <#proxy_path as super::wayrs_client::proxy::Proxy>::interface(),
                            version: <#proxy_path as super::wayrs_client::proxy::Proxy>::interface().version,
                       })
                    }
                },
                ("string", None) => quote! { super::wayrs_client::wire::ArgValue::String(#arg_name.into()) },
                ("array", None) => quote! { super::wayrs_client::wire::ArgValue::Array(#arg_name) },
                ("fd", None) => quote! { super::wayrs_client::wire::ArgValue::Fd(#arg_name) },
                ("enum", None) => todo!(),
                _ => unreachable!(),
            }
        });

        let send_message = quote! {
            event_queue.connection().send_request(
                &#static_name,
                super::wayrs_client::wire::Message {
                    header: super::wayrs_client::wire::MessageHeader {
                        object_id: self.iner.id,
                        size: 0,
                        opcode: #opcode,
                    },
                    args: vec![ #( #msg_args, )* ],
                }
            );
        };

        let (generics, ret_ty) = match (new_id_cnt, new_id_interface) {
            (0, _) => (quote! { D: super::wayrs_client::proxy::Dispatcher }, quote! { () }),
            (1, Some(i)) => {
                let proxy_path = make_proxy_path(i);
                (
                    quote! { D: super::wayrs_client::proxy::Dispatch<#proxy_path> },
                    proxy_path
                )
            }
            (1, None) => (
                quote! { P: super::wayrs_client::proxy::Proxy, D: super::wayrs_client::proxy::Dispatch<P> },
                quote! { P },
            ),
            _ => panic!("request with more than one new_id?"),
        };

        let body = match (new_id_cnt, new_id_interface) {
            (0, _) => send_message,
            (1, Some(_)) => {
                quote! {
                    event_queue.set_callback::<#ret_ty>();
                    let new_object = #send_message
                    new_object.unwrap().try_into().unwrap()
                }
            }
            (1, None) => quote! {
                assert!(version <= <P as super::wayrs_client::proxy::Proxy>::interface().version);
                event_queue.set_callback::<P>();
                let new_object = #send_message
                new_object.unwrap().try_into().unwrap()
            },
            _ => panic!("request with more than one new_id?"),
        };

        quote! {
            #[allow(clippy::too_many_arguments)]
            pub fn #request_name<#generics>(
                &self, event_queue: &mut super::wayrs_client::event_queue::EventQueue<D> #( #fn_args )*
            ) -> #ret_ty {
                #body
            }
        }
    });

    let enums = iface.enums.iter().map(|en| {
        let name = make_pascal_case_ident(&en.name);
        let items = en
            .items
            .iter()
            .map(|item| make_pascal_case_ident(&item.name));
        let values = en.items.iter().map(|item| item.value);
        let items2 = items.clone();
        let values2 = values.clone();
        if en.is_bitfield {
            quote! {
                #[derive(Debug, Clone, Copy)]
                pub struct #name(u32);
                impl From<#name> for u32 {
                    fn from(val: #name) -> Self {
                        val.0
                    }
                }
                impl From<u32> for #name {
                    fn from(val: u32) -> Self {
                        Self(val)
                    }
                }
                impl #name {
                    #( pub const #items: Self = Self(#values); )*
                    pub fn contains(self, item: Self) -> bool {
                        self.0 & item.0 != 0
                    }
                }
                impl ::std::ops::BitOr for #name {
                    type Output = Self;
                    fn bitor(self, rhs: Self) -> Self {
                        Self(self.0 | rhs.0)
                    }
                }
            }
        } else {
            quote! {
                #[repr(u32)]
                #[derive(Debug, Clone, Copy, PartialEq, Eq)]
                pub enum #name { #( #items = #values, )* }
                impl From<#name> for u32 {
                    fn from(val: #name) -> u32 {
                        val as u32
                    }
                }
                impl From<u32> for #name {
                    fn from(val: u32) -> Self {
                        match val {
                            #( #values2 => Self::#items2, )*
                            _ => unreachable!()
                        }
                    }
                }
            }
        }
    });

    quote! {
        pub static #static_name: &super::wayrs_client::interface::Interface = &super::wayrs_client::interface::Interface {
            name: super::wayrs_client::cstr!(#raw_name),
            version: #version,
            events: &[ #(#events_desc,)* ],
            requests: &[ #(#requests_desc,)* ],
        };

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct #proxy_name {
            iner: super::wayrs_client::object::Object,
        }

        impl super::wayrs_client::proxy::Proxy for #proxy_name {
            type Event = Event;

            fn interface() -> &'static super::wayrs_client::interface::Interface {
                #static_name
            }

            fn id(&self) -> super::wayrs_client::object::ObjectId {
                self.iner.id
            }

            fn version(&self) -> u32 {
                self.iner.version
            }
        }

        impl TryFrom<super::wayrs_client::object::Object> for #proxy_name {
            type Error = super::wayrs_client::proxy::WrongObject;

            fn try_from(object: super::wayrs_client::object::Object) -> Result<Self, Self::Error> {
                if object.interface == #static_name {
                    Ok(Self { iner: object })
                } else {
                    Err(super::wayrs_client::proxy::WrongObject)
                }
            }
        }

        #( #event_args_structs )*
        #( #enums )*

        #[derive(Debug)]
        pub enum Event {
            #( #event_enum_options, )*
        }

        impl TryFrom<super::wayrs_client::wire::Message> for Event {
            type Error = super::wayrs_client::proxy::BadMessage;

            fn try_from(event: super::wayrs_client::wire::Message) -> Result<Self, Self::Error> {
                match event.header.opcode {
                    #( #event_decoding )*
                    _ => Err(super::wayrs_client::proxy::BadMessage),
                }
            }
        }

        impl #proxy_name {
            #( #requests )*
        }
    }
}

fn map_arg_to_sturt_field(arg: &Argument) -> TokenStream {
    let name = make_ident(&arg.name);
    let ty = map_arg_to_rs(arg);
    quote! { #name: #ty }
}

fn map_arg_to_argtype(arg: &Argument) -> TokenStream {
    match arg.arg_type.as_str() {
        "int" => quote!(Int),
        "uint" => quote!(Uint),
        "fixed" => quote!(Fixed),
        "object" => quote!(Object),
        "new_id" => match &arg.interface {
            Some(iface) => {
                let iface_name = syn::Ident::new(iface, Span::call_site());
                let static_name =
                    syn::Ident::new(&(iface.to_uppercase() + "_INTERFACE"), Span::call_site());
                quote! { NewId(super::#iface_name::#static_name) }
            }
            None => quote!(AnyNewId),
        },
        "string" => quote!(String),
        "array" => quote!(Array),
        "fd" => quote!(Fd),
        "enum" => quote!(Enum),
        _ => unreachable!(),
    }
}

fn map_arg_to_argval(arg: &Argument) -> TokenStream {
    match arg.arg_type.as_str() {
        "int" => quote!(Int),
        "uint" => quote!(Uint),
        "fixed" => quote!(Fixed),
        "object" => quote!(Object),
        "new_id" => quote!(NewId),
        "string" => quote!(String),
        "array" => quote!(Array),
        "fd" => quote!(Fd),
        "enum" => quote!(Enum),
        _ => unreachable!(),
    }
}

fn map_arg_to_rs(arg: &Argument) -> TokenStream {
    match arg.arg_type.as_str() {
        "int" => quote!(i32),
        "uint" => {
            if let Some(enum_type) = &arg.enum_type {
                if let Some((iface, name)) = enum_type.split_once('.') {
                    let iface_name = syn::Ident::new(iface, Span::call_site());
                    let enum_name = make_pascal_case_ident(name);
                    quote!(super::#iface_name::#enum_name)
                } else {
                    let enum_name = make_pascal_case_ident(enum_type);
                    quote!(#enum_name)
                }
            } else {
                quote!(u32)
            }
        }
        "fixed" => quote!(super::wayrs_client::wire::Fixed),
        "object" => quote!(super::wayrs_client::object::ObjectId),
        "new_id" => {
            if let Some(iface) = &arg.interface {
                make_proxy_path(iface)
            } else {
                quote!(super::wayrs_client::object::Object)
            }
        }
        "string" => quote!(::std::ffi::CString),
        "array" => quote!(::std::vec::Vec<u8>),
        "fd" => quote!(::std::os::unix::io::OwnedFd),
        _ => unreachable!(),
    }
}

fn gen_doc(desc: &Option<Description>) -> TokenStream {
    let summary = desc.as_ref().and_then(|d| d.summary.as_deref());
    let doc: Option<String> = desc
        .as_ref()
        .and_then(|d| d.text.as_deref())
        .map(|d| d.lines().flat_map(|line| [line.trim(), "\n"]))
        .map(|it| it.collect());
    match (summary, doc) {
        (Some(s), Some(d)) => quote! {
            #[doc = #s]
            #[doc = "\n"]
            #[doc = #d]
        },
        (Some(doc), None) => quote! {
            #[doc = #doc]
        },
        (None, Some(doc)) => quote! {
            #[doc = #doc]
        },
        (None, None) => quote!(),
    }
}
