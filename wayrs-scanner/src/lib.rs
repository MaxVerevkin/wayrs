mod parser;
mod types;

use std::path::PathBuf;

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};

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

    let modules = protocol.interfaces.iter().map(gen_interface);
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
    let proxy_name = make_pascal_case_ident(iface);
    quote! { super::#proxy_name }
}

fn gen_interface(iface: &Interface) -> TokenStream {
    let mod_doc = gen_doc(&iface.description, None);
    let mod_name = syn::Ident::new(&iface.name, Span::call_site());

    let proxy_name = make_pascal_case_ident(&iface.name);

    let raw_name = &iface.name;
    let version = iface.version;

    let gen_msg_gesc = |msg: &Message| {
        let args = msg.args.iter().map(map_arg_to_argtype);
        let name = &msg.name;
        let is_destructor = msg.kind.as_deref() == Some("destructor");
        quote! {
            wayrs_client::interface::MessageDesc {
                name: #name,
                is_destructor: #is_destructor,
                signature: &[ #( wayrs_client::wire::ArgType::#args, )* ]
            }
        }
    };
    let events_desc = iface.events.iter().map(gen_msg_gesc);
    let requests_desc = iface.requests.iter().map(gen_msg_gesc);

    let event_args_structs = iface
        .events
        .iter()
        .filter(|event| event.args.len() > 1)
        .map(|event| {
            let struct_name = format_ident!("{}Args", make_pascal_case_ident(&event.name));
            let arg_name = event.args.iter().map(|arg| make_ident(&arg.name));
            let arg_ty = event.args.iter().map(map_arg_to_rs);
            quote! {
                #[derive(Debug)]
                pub struct #struct_name { #( pub #arg_name: #arg_ty, )* }
            }
        });

    let event_enum_options = iface.events.iter().map(|event| {
        let event_name = make_pascal_case_ident(&event.name);
        let doc = gen_doc(&event.description, Some(event.since));
        match event.args.as_slice() {
            [] => quote! { #doc #event_name },
            [_, _, ..] => {
                let struct_name = format_ident!("{event_name}Args");
                quote! { #doc #event_name(#struct_name) }
            }
            [arg] => {
                let event_ty = map_arg_to_rs(arg);
                quote! { #doc #event_name(#event_ty) }
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
        let arg_decode = event.args.iter().map(|arg| {
            let arg_name = make_ident(&arg.name);
            match arg.arg_type.as_str() {
                "new_id" => {
                    let iface = arg.interface.as_deref().unwrap();
                    let iface_name = syn::Ident::new(iface, Span::call_site());
                    quote! {
                        {
                            let obj = wayrs_client::object::Object {
                                id: #arg_name,
                                version: self.version(),
                                interface: super::#iface_name::INTERFACE,
                            };
                            obj.try_into().unwrap()
                        }
                    }
                }
                _ => quote!(#arg_name.try_into().unwrap()),
            }
        });
        let args_len = event.args.len();
        let retval = match args_len {
            0 => quote!(Event::#event_name),
            1 => quote!(Event::#event_name(#( #arg_decode )*)),
            _ => quote!(Event::#event_name(#struct_name { #( #arg_names2: #arg_decode, )* })),
        };
        quote! {
            #opcode => {
                if event.args.len() != #args_len {
                    return Err(wayrs_client::proxy::BadMessage);
                }
                let mut args = event.args.into_iter();
                #( let Some(wayrs_client::wire::ArgValue::#arg_ty(#arg_names)) = args.next() else { return Err(wayrs_client::proxy::BadMessage) }; )*
                Ok(#retval)
            }
        }
    });

    let requests = iface
        .requests
        .iter()
        .enumerate()
        .map(|(opcode, request)| gen_request_fn(opcode as u16, request));

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
        #mod_doc
        pub mod #mod_name {
            use super::wayrs_client;
            use super::wayrs_client::proxy::Proxy;
            use super::wayrs_client::connection::Connection;

            pub static INTERFACE: &wayrs_client::interface::Interface = &wayrs_client::interface::Interface {
                name: wayrs_client::cstr!(#raw_name),
                version: #version,
                events: &[ #(#events_desc,)* ],
                requests: &[ #(#requests_desc,)* ],
            };

            #mod_doc
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
            pub struct #proxy_name {
                iner: wayrs_client::object::Object,
            }

            impl Proxy for #proxy_name {
                type Event = Event;

                fn interface() -> &'static wayrs_client::interface::Interface {
                    INTERFACE
                }

                fn null() -> Self {
                    Self {
                        iner: wayrs_client::object::Object {
                            id: wayrs_client::object::ObjectId::NULL,
                            version: 0,
                            interface: INTERFACE,
                        }
                    }
                }

                fn parse_event(&self, event: wayrs_client::wire::Message) -> Result<Event, wayrs_client::proxy::BadMessage> {
                    match event.header.opcode {
                        #( #event_decoding )*
                        _ => Err(wayrs_client::proxy::BadMessage),
                    }
                }

                fn id(&self) -> wayrs_client::object::ObjectId {
                    self.iner.id
                }

                fn version(&self) -> u32 {
                    self.iner.version
                }
            }

            impl TryFrom<wayrs_client::object::Object> for #proxy_name {
                type Error = wayrs_client::proxy::WrongObject;

                fn try_from(object: wayrs_client::object::Object) -> Result<Self, Self::Error> {
                    if object.interface == INTERFACE {
                        Ok(Self { iner: object })
                    } else {
                        Err(wayrs_client::proxy::WrongObject)
                    }
                }
            }

            impl From<#proxy_name> for wayrs_client::object::Object {
                fn from(proxy: #proxy_name) -> Self {
                    proxy.iner
                }
            }

            impl From<#proxy_name> for wayrs_client::object::ObjectId {
                fn from(proxy: #proxy_name) -> Self {
                    proxy.iner.id
                }
            }

            #( #event_args_structs )*
            #( #enums )*

            #[derive(Debug)]
            pub enum Event {
                #( #event_enum_options, )*
            }

            impl #proxy_name {
                #( #requests )*
            }
        }

        pub use #mod_name::#proxy_name;
    }
}

fn gen_pub_fn(
    attrs: &TokenStream,
    name: &str,
    generics: &[TokenStream],
    args: &[TokenStream],
    ret_ty: TokenStream,
    body: TokenStream,
) -> TokenStream {
    let name = make_ident(name);
    quote! {
        #attrs
        #[allow(clippy::too_many_arguments)]
        pub fn #name<#(#generics),*>(#(#args),*) -> #ret_ty {
            #body
        }
    }
}

fn gen_request_fn(opcode: u16, request: &Message) -> TokenStream {
    assert!(
        request
            .args
            .iter()
            .filter(|x| x.arg_type == "new_id")
            .count()
            <= 1,
        "{} has more than one new_id argument",
        request.name,
    );

    let new_id_interface = request
        .args
        .iter()
        .find(|x| x.arg_type == "new_id")
        .map(|x| x.interface.as_deref());

    let mut fn_args = vec![quote!(&self), quote!(conn: &mut Connection<D>)];
    for arg in &request.args {
        let arg_name = make_ident(&arg.name);
        let arg_ty = map_arg_to_rs(arg);
        match (arg.arg_type.as_str(), arg.interface.as_deref()) {
            ("int" | "uint" | "fixed" | "string" | "array" | "fd", None) => {
                fn_args.push(quote!(#arg_name: #arg_ty));
            }
            ("object", None) => fn_args.push(quote!(#arg_name: wayrs_client::object::Object)),
            ("object", Some(i)) => {
                let proxy_path = make_proxy_path(i);
                fn_args.push(quote!(#arg_name: #proxy_path));
            }
            ("new_id", None) => fn_args.push(quote!(version: u32)),
            ("new_id", Some(_)) => (),
            _ => unreachable!(),
        }
    }

    let msg_args = request.args.iter().map(|arg| {
        let arg_name = make_ident(&arg.name);
        let arg_ty = map_arg_to_argval(arg);
        match arg.arg_type.as_str() {
            "new_id" => quote! { wayrs_client::wire::ArgValue::#arg_ty(new_object.into()) },
            _ => quote! { wayrs_client::wire::ArgValue::#arg_ty(#arg_name.into()) },
        }
    });

    let send_message = quote! {
        conn.send_request(
            &INTERFACE,
            wayrs_client::wire::Message {
                header: wayrs_client::wire::MessageHeader {
                    object_id: self.id(),
                    size: 0,
                    opcode: #opcode,
                },
                args: vec![ #( #msg_args, )* ],
            }
        );
    };

    let doc = gen_doc(&request.description, Some(request.since));

    match new_id_interface {
        None => gen_pub_fn(
            &doc,
            &request.name,
            &[quote!(D)],
            &fn_args,
            quote!(()),
            send_message,
        ),
        Some(None) => {
            let no_cb = gen_pub_fn(
                &doc,
                &request.name,
                &[quote!(P: Proxy), quote!(D)],
                &fn_args,
                quote!(P),
                quote! {
                    let new_object = conn.allocate_new_object::<P>(version);
                    #send_message
                    new_object
                },
            );
            fn_args.push(quote!(cb: F));
            let cb = gen_pub_fn(
                &doc,
                &format!("{}_with_cb", request.name),
                &[
                    quote!(P: Proxy),
                    quote!(D),
                    quote!(F: FnMut(&mut Connection<D>, &mut D, P, <P as Proxy>::Event) + Send + 'static),
                ],
                &fn_args,
                quote!(P),
                quote! {
                    let new_object = conn.allocate_new_object_with_cb::<P, F>(version, cb);
                    #send_message
                    new_object
                },
            );
            quote! {
                #no_cb
                #cb
            }
        }
        Some(Some(i)) => {
            let proxy_path = make_proxy_path(i);
            let no_cb = gen_pub_fn(
                &doc,
                &request.name,
                &[quote!(D)],
                &fn_args,
                proxy_path.clone(),
                quote! {
                    let new_object = conn.allocate_new_object::<#proxy_path>(self.version());
                    #send_message
                    new_object
                },
            );
            fn_args.push(quote!(cb: F));
            let cb = gen_pub_fn(
                &doc,
                &format!("{}_with_cb", request.name),
                &[
                    quote!(D),
                    quote!(F: FnMut(&mut Connection<D>, &mut D, #proxy_path, <#proxy_path as Proxy>::Event) + Send + 'static),
                ],
                &fn_args,
                proxy_path.clone(),
                quote! {
                    let new_object = conn.allocate_new_object_with_cb::<#proxy_path, F>(self.version(), cb);
                    #send_message
                    new_object
                },
            );
            quote! {
                #no_cb
                #cb
            }
        }
    }
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
                quote!(NewId(super::#iface_name::INTERFACE))
            }
            None => quote!(AnyNewId),
        },
        "string" => quote!(String),
        "array" => quote!(Array),
        "fd" => quote!(Fd),
        _ => unreachable!(),
    }
}

fn map_arg_to_argval(arg: &Argument) -> TokenStream {
    match arg.arg_type.as_str() {
        "int" => quote!(Int),
        "uint" => quote!(Uint),
        "fixed" => quote!(Fixed),
        "object" => quote!(Object),
        "new_id" => match arg.interface.as_deref() {
            Some(_) => quote!(NewId),
            None => quote!(AnyNewId),
        },
        "string" => quote!(String),
        "array" => quote!(Array),
        "fd" => quote!(Fd),
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
        "fixed" => quote!(wayrs_client::wire::Fixed),
        "object" => quote!(wayrs_client::object::ObjectId),
        "new_id" => {
            if let Some(iface) = &arg.interface {
                make_proxy_path(iface)
            } else {
                quote!(wayrs_client::object::Object)
            }
        }
        "string" => quote!(::std::ffi::CString),
        "array" => quote!(::std::vec::Vec<u8>),
        "fd" => quote!(::std::os::unix::io::OwnedFd),
        _ => unreachable!(),
    }
}

fn gen_doc(desc: &Option<Description>, since: Option<u32>) -> TokenStream {
    let summary = desc.as_ref().and_then(|d| d.summary.as_deref());
    let since = since.map(|ver| format!("\n**Since version {ver}**."));
    let doc: Option<String> = desc
        .as_ref()
        .and_then(|d| d.text.as_deref())
        .map(|d| {
            d.lines()
                .flat_map(|line| [line.trim(), "\n"])
                .chain(since.as_deref())
        })
        .map(|it| it.collect())
        .or(since);
    match (summary, doc.as_deref()) {
        (Some(s), Some(d)) => quote! {
            #[doc = #s]
            #[doc = "\n"]
            #[doc = #d]
        },
        (Some(doc), None) | (None, Some(doc)) => quote! {
            #[doc = #doc]
        },
        (None, None) => quote!(),
    }
}
