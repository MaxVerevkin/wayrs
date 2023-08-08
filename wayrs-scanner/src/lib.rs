mod parser;
mod types;
mod utils;

use std::path::PathBuf;

use proc_macro2::{Span, TokenStream};
use proc_macro_crate::{crate_name, FoundCrate};
use quote::{format_ident, quote};

use crate::parser::Parser;
use crate::types::*;
use crate::utils::*;

fn wayrs_client_path() -> TokenStream {
    match crate_name("wayrs-client") {
        Ok(FoundCrate::Name(name)) => {
            let ident = format_ident!("{}", name);
            quote! { ::#ident }
        }
        Ok(FoundCrate::Itself) => quote! { crate },
        _ => quote! { ::wayrs_client },
    }
}

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

    let wayrs_client_path = wayrs_client_path();
    let modules = protocol
        .interfaces
        .iter()
        .map(|i| gen_interface(i, &wayrs_client_path));

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
        syn::Ident::new_raw(&format!("_{name}"), Span::call_site())
    } else {
        syn::Ident::new_raw(&snake_to_pascal(name), Span::call_site())
    }
}

fn make_proxy_path(iface: impl AsRef<str>) -> TokenStream {
    let proxy_name = make_pascal_case_ident(iface);
    quote! { super::#proxy_name }
}

fn gen_interface(iface: &Interface, wayrs_client_path: &TokenStream) -> TokenStream {
    let mod_doc = gen_doc(&iface.description, None);
    let mod_name = syn::Ident::new(&iface.name, Span::call_site());

    let proxy_name = make_pascal_case_ident(&iface.name);

    let raw_iface_name = &iface.name;
    let iface_version = iface.version;

    let gen_msg_gesc = |msg: &Message| {
        let args = msg.args.iter().map(map_arg_to_argtype);
        let name = &msg.name;
        let is_destructor = msg.kind.as_deref() == Some("destructor");
        quote! {
            _wayrs_client::interface::MessageDesc {
                name: #name,
                is_destructor: #is_destructor,
                signature: &[ #( _wayrs_client::wire::ArgType::#args, )* ]
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
            let arg_ty = event.args.iter().map(|arg| arg.as_event_ty());
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
                let event_ty = arg.as_event_ty();
                quote! { #doc #event_name(#event_ty) }
            }
        }
    });

    let event_decoding = iface.events.iter().enumerate().map(|(opcode, event)| {
        let event_name = make_pascal_case_ident(&event.name);
        let opcode = opcode as u16;
        let arg_ty = event.args.iter().map(|x| map_arg_to_argval(x, true));
        let arg_names = event.args.iter().map(|arg| make_ident(&arg.name));
        let arg_decode = event.args.iter().map(|arg| {
            let arg_name = make_ident(&arg.name);
            match (arg.arg_type.as_str(), arg.enum_type.is_some()) {
                ("new_id", _) | ("int" | "uint", true) => quote! {
                    match #arg_name.try_into() {
                        Ok(val) => val,
                        Err(_) => return Err(_wayrs_client::proxy::BadMessage),
                    }
                },
                _ => quote!(#arg_name),
            }
        });
        let args_len = event.args.len();
        let retval = match args_len {
            0 => quote!(Event::#event_name),
            1 => quote!(Event::#event_name(#( #arg_decode )*)),
            _ => {
                let struct_name = format_ident!("{event_name}Args");
                let arg_names = arg_names.clone();
                quote!(Event::#event_name(#struct_name { #( #arg_names: #arg_decode, )* }))
            }
        };
        quote! {
            #opcode => {
                if event.args.len() != #args_len {
                    return Err(_wayrs_client::proxy::BadMessage);
                }
                let mut args = event.args.into_iter();
                #( let Some(_wayrs_client::wire::ArgValue::#arg_ty(#arg_names)) = args.next() else { return Err(_wayrs_client::proxy::BadMessage) }; )*
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
        let doc = gen_doc(&en.description, None);
        let item_docs = en
            .items
            .iter()
            .map(|i| gen_doc(&i.description, Some(i.since)));
        if en.is_bitfield {
            quote! {
                #doc
                #[derive(Debug, Default, Clone, Copy)]
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
                    #(
                        #item_docs
                        #[allow(non_upper_case_globals)]
                        pub const #items: Self = Self(#values);
                    )*

                    pub fn empty() -> Self {
                        Self(0)
                    }
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
                #doc
                #[repr(u32)]
                #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
                #[non_exhaustive]
                pub enum #name { #( #item_docs #items = #values, )* }
                impl From<#name> for u32 {
                    fn from(val: #name) -> u32 {
                        val as u32
                    }
                }
                impl TryFrom<u32> for #name {
                    type Error = ();
                    fn try_from(val: u32) -> ::std::result::Result<Self, ()> {
                        match val {
                            #( #values2 => Ok(Self::#items2), )*
                            _ => Err(()),
                        }
                    }
                }
            }
        }
    });

    let visibility = if iface.name == "wl_display" {
        quote!(pub(crate))
    } else {
        quote!(pub)
    };

    let extra_impl = if iface.name == "wl_display" {
        quote! {
            impl WlDisplay {
                pub const INSTANCE: Self = Self {
                    id: _wayrs_client::object::ObjectId::DISPLAY,
                    version: 1,
                };
            }
        }
    } else {
        quote!()
    };

    quote! {
        #mod_doc
        #visibility mod #mod_name {
            use #wayrs_client_path as _wayrs_client;
            use _wayrs_client::proxy::Proxy;

            #mod_doc
            #[derive(Clone, Copy)]
            pub struct #proxy_name {
                id: _wayrs_client::object::ObjectId,
                version: u32,
            }

            #extra_impl

            impl Proxy for #proxy_name {
                type Event = Event;

                const INTERFACE: &'static _wayrs_client::interface::Interface
                    = &_wayrs_client::interface::Interface {
                        name: _wayrs_client::cstr!(#raw_iface_name),
                        version: #iface_version,
                        events: &[ #(#events_desc,)* ],
                        requests: &[ #(#requests_desc,)* ],
                    };

                fn new(id: _wayrs_client::object::ObjectId, version: u32) -> Self {
                    Self { id, version }
                }

                fn id(&self) -> _wayrs_client::object::ObjectId {
                    self.id
                }

                fn version(&self) -> u32 {
                    self.version
                }
            }

            impl TryFrom<_wayrs_client::wire::Message> for Event {
                type Error = _wayrs_client::proxy::BadMessage;

                fn try_from(event: _wayrs_client::wire::Message) -> ::std::result::Result<Self, _wayrs_client::proxy::BadMessage> {
                    match event.header.opcode {
                        #( #event_decoding )*
                        _ => Err(_wayrs_client::proxy::BadMessage),
                    }
                }
            }

            impl TryFrom<_wayrs_client::object::Object> for #proxy_name {
                type Error = _wayrs_client::proxy::WrongObject;

                fn try_from(object: _wayrs_client::object::Object) -> ::std::result::Result<Self, _wayrs_client::proxy::WrongObject> {
                    if object.interface == Self::INTERFACE {
                        Ok(Self {
                            id: object.id,
                            version: object.version,
                        })
                    } else {
                        Err(_wayrs_client::proxy::WrongObject)
                    }
                }
            }

            impl ::std::fmt::Debug for #proxy_name {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    write!(
                        f,
                        "{}@{}v{}",
                        #raw_iface_name,
                        self.id.as_u32(),
                        self.version
                    )
                }
            }

            impl ::std::cmp::PartialEq for #proxy_name {
                fn eq(&self, other: &Self) -> bool {
                    self.id == other.id
                }
            }

            impl ::std::cmp::Eq for #proxy_name {}

            impl ::std::cmp::PartialOrd for #proxy_name {
                fn partial_cmp(&self, other: &Self) -> ::std::option::Option<::std::cmp::Ordering> {
                    ::std::option::Option::Some(::std::cmp::Ord::cmp(self, other))
                }
            }

            impl ::std::cmp::Ord for #proxy_name {
                fn cmp(&self, other: &Self) -> ::std::cmp::Ordering {
                    self.id.cmp(&other.id)
                }
            }

            impl ::std::hash::Hash for #proxy_name {
                fn hash<H>(&self, state: &mut H)
                    where H: ::std::hash::Hasher
                {
                    self.id.hash(state);
                }
            }

            #( #event_args_structs )*
            #( #enums )*

            #[derive(Debug)]
            #[non_exhaustive]
            pub enum Event {
                #( #event_enum_options, )*
            }

            impl #proxy_name {
                #( #requests )*
            }
        }

        #visibility use #mod_name::#proxy_name;
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

    let mut fn_args = vec![
        quote!(self),
        quote!(conn: &mut _wayrs_client::Connection<D>),
    ];
    fn_args.extend(request.args.iter().flat_map(|arg| arg.as_request_fn_arg()));

    let msg_args = request.args.iter().map(|arg| {
        let arg_name = make_ident(&arg.name);
        let arg_ty = map_arg_to_argval(arg, false);
        match arg.arg_type.as_str() {
            "new_id" => quote! { _wayrs_client::wire::ArgValue::#arg_ty(new_object.into()) },
            "object" if arg.allow_null => {
                quote! { _wayrs_client::wire::ArgValue::#arg_ty(#arg_name.map(Into::into)) }
            }
            _ => quote! { _wayrs_client::wire::ArgValue::#arg_ty(#arg_name.into()) },
        }
    });

    let send_message = quote! {
        conn.send_request(
            Self::INTERFACE,
            _wayrs_client::wire::Message {
                header: _wayrs_client::wire::MessageHeader {
                    object_id: self.id,
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
                    quote!(F: FnMut(&mut _wayrs_client::Connection<D>, &mut D, P, <P as Proxy>::Event) + Send + 'static),
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
                    let new_object = conn.allocate_new_object::<#proxy_path>(self.version);
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
                    quote!(F: FnMut(&mut _wayrs_client::Connection<D>, &mut D, #proxy_path, <#proxy_path as Proxy>::Event) + Send + 'static),
                ],
                &fn_args,
                proxy_path.clone(),
                quote! {
                    let new_object = conn.allocate_new_object_with_cb::<#proxy_path, F>(self.version, cb);
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
        "int" if arg.enum_type.is_some() => quote!(Uint),
        "int" => quote!(Int),
        "uint" => quote!(Uint),
        "fixed" => quote!(Fixed),
        "object" => match arg.allow_null {
            false => quote!(Object),
            true => quote!(OptObject),
        },
        "new_id" => match &arg.interface {
            Some(iface) => {
                let proxy_name = make_proxy_path(iface);
                quote!(NewId(#proxy_name::INTERFACE))
            }
            None => quote!(AnyNewId),
        },
        "string" => match arg.allow_null {
            false => quote!(String),
            true => quote!(OptString),
        },
        "array" => quote!(Array),
        "fd" => quote!(Fd),
        _ => unreachable!(),
    }
}

fn map_arg_to_argval(arg: &Argument, is_event: bool) -> TokenStream {
    match arg.arg_type.as_str() {
        "int" if arg.enum_type.is_some() => quote!(Uint),
        "int" => quote!(Int),
        "uint" => quote!(Uint),
        "fixed" => quote!(Fixed),
        "object" => match arg.allow_null {
            false => quote!(Object),
            true => quote!(OptObject),
        },
        "new_id" if is_event => match arg.interface.as_deref() {
            Some(_) => quote!(NewIdEvent),
            None => unimplemented!(),
        },
        "new_id" if !is_event => match arg.interface.as_deref() {
            Some(_) => quote!(NewIdRequest),
            None => quote!(AnyNewIdRequest),
        },
        "string" => match arg.allow_null {
            false => quote!(String),
            true => quote!(OptString),
        },
        "array" => quote!(Array),
        "fd" => quote!(Fd),
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

trait ArgExt {
    fn as_request_fn_arg(&self) -> Option<TokenStream>;
    fn as_event_ty(&self) -> TokenStream;
}

impl ArgExt for Argument {
    fn as_request_fn_arg(&self) -> Option<TokenStream> {
        let arg_name = make_ident(&self.name);
        let retval = match (
            self.arg_type.as_str(),
            self.interface.as_deref(),
            self.enum_type.as_deref(),
            self.allow_null,
        ) {
            ("int", None, None, false) => quote!(#arg_name: i32),
            ("uint", None, None, false) => quote!(#arg_name: u32),
            ("int" | "uint", None, Some(enum_ty), false) => {
                if let Some((iface, name)) = enum_ty.split_once('.') {
                    let iface_name = syn::Ident::new(iface, Span::call_site());
                    let enum_name = make_pascal_case_ident(name);
                    quote!(#arg_name: super::#iface_name::#enum_name)
                } else {
                    let enum_name = make_pascal_case_ident(enum_ty);
                    quote!(#arg_name: #enum_name)
                }
            }
            ("fixed", None, None, false) => quote!(#arg_name: _wayrs_client::wire::Fixed),
            ("object", None, None, allow_null) => match allow_null {
                false => quote!(#arg_name: _wayrs_client::object::Object),
                true => quote!(#arg_name: ::std::option::Option<_wayrs_client::object::Object>),
            },
            ("object", Some(iface), None, allow_null) => {
                let proxy_path = make_proxy_path(iface);
                match allow_null {
                    false => quote!(#arg_name: #proxy_path),
                    true => quote!(#arg_name: ::std::option::Option<#proxy_path>),
                }
            }
            ("new_id", None, None, false) => quote!(version: u32),
            ("new_id", Some(_), None, false) => return None,
            ("string", None, None, allow_null) => match allow_null {
                false => quote!(#arg_name: ::std::ffi::CString),
                true => quote!(#arg_name: ::std::option::Option<::std::ffi::CString>),
            },
            ("array", None, None, false) => quote!(#arg_name: ::std::vec::Vec<u8>),
            ("fd", None, None, false) => quote!(#arg_name: ::std::os::unix::io::OwnedFd),
            _ => unreachable!(),
        };
        Some(retval)
    }

    fn as_event_ty(&self) -> TokenStream {
        match self.arg_type.as_str() {
            "int" | "uint" if self.enum_type.is_some() => {
                let enum_type = self.enum_type.as_deref().unwrap();
                if let Some((iface, name)) = enum_type.split_once('.') {
                    let iface_name = syn::Ident::new(iface, Span::call_site());
                    let enum_name = make_pascal_case_ident(name);
                    quote!(super::#iface_name::#enum_name)
                } else {
                    let enum_name = make_pascal_case_ident(enum_type);
                    quote!(#enum_name)
                }
            }
            "int" => quote!(i32),
            "uint" => quote!(u32),
            "fixed" => quote!(_wayrs_client::wire::Fixed),
            "object" => match self.allow_null {
                false => quote!(_wayrs_client::object::ObjectId),
                true => quote!(::std::option::Option<_wayrs_client::object::ObjectId>),
            },
            "new_id" => {
                if let Some(iface) = &self.interface {
                    make_proxy_path(iface)
                } else {
                    quote!(_wayrs_client::object::Object)
                }
            }
            "string" => match self.allow_null {
                false => quote!(::std::ffi::CString),
                true => quote!(::std::option::Option<::std::ffi::CString>),
            },
            "array" => quote!(::std::vec::Vec<u8>),
            "fd" => quote!(::std::os::unix::io::OwnedFd),
            _ => unreachable!(),
        }
    }
}
