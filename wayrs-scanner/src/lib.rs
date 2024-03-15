//! Generate glue code from .xml files for `wayrs-client`.
//!
//! **Do not use directly in your projcets. Call `wayrs_client::generate!()` instead.**

use std::path::PathBuf;

use proc_macro2::{Span, TokenStream};
use proc_macro_crate::{crate_name, FoundCrate};
use quote::{format_ident, quote};
use wayrs_proto_parser::*;

mod utils;
use crate::utils::*;

/// These interfaces are frozen at version 1 and will not introduce new events or requests.
const FROZEN_IFACES: &[&str] = &["wl_callback", "wl_buffer"];

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

#[doc(hidden)]
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
    let protocol = match parse_protocol(&file) {
        Ok(protocol) => protocol,
        Err(err) => {
            let err = format!("error parsing the protocol file: {err}");
            return quote!(compile_error!(#err);).into();
        }
    };

    let wayrs_client_path = wayrs_client_path();
    let modules = protocol
        .interfaces
        .iter()
        .map(|i| gen_interface(i, &wayrs_client_path));

    let x = quote! { #(#modules)* };
    // {
    //     let mut file = std::fs::File::create("/tmp/test.rs").unwrap();
    //     std::io::Write::write_all(&mut file, x.to_string().as_bytes()).unwrap();
    // }
    x.into()
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
    let mod_doc = gen_doc(iface.description.as_ref(), None);
    let mod_name = syn::Ident::new(&iface.name, Span::call_site());

    let proxy_name = make_pascal_case_ident(&iface.name);
    let proxy_name_str = snake_to_pascal(&iface.name);

    let raw_iface_name = &iface.name;
    let iface_version = iface.version;

    let gen_msg_gesc = |msg: &Message| {
        let args = msg.args.iter().map(map_arg_to_argtype);
        let name = &msg.name;
        let is_destructor = msg.kind.as_deref() == Some("destructor");
        quote! {
            _wayrs_client::core::MessageDesc {
                name: #name,
                is_destructor: #is_destructor,
                signature: &[ #( _wayrs_client::core::ArgType::#args, )* ]
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
            let summary = event
                .args
                .iter()
                .map(|arg| arg.summary.as_ref().map(|s| quote!(#[doc = #s])));
            quote! {
                #[derive(Debug)]
                pub struct #struct_name { #( #summary pub #arg_name: #arg_ty, )* }
            }
        });

    let event_enum_options = iface.events.iter().map(|event| {
        let event_name = make_pascal_case_ident(&event.name);
        let doc = gen_doc(event.description.as_ref(), Some(event.since));
        match event.args.as_slice() {
            [] => quote! { #doc #event_name },
            [_, _, ..] => {
                let struct_name = format_ident!("{event_name}Args");
                quote! { #doc #event_name(#struct_name) }
            }
            [arg] => {
                let event_ty = arg.as_event_ty();
                let arg_name = &arg.name;
                let name_doc = quote!(#[doc = #arg_name]);
                let summary = arg
                    .summary
                    .as_ref()
                    .map(|s| quote!(#[doc = "\n"] #[doc = #s]));
                quote! { #doc #event_name(#name_doc #summary #event_ty) }
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
            match &arg.arg_type {
                ArgType::NewId{iface: Some(iface)} => {
                    let proxy_name = make_proxy_path(iface);
                    quote! {
                        <#proxy_name as Proxy>::new(#arg_name, __self_version)
                    }
                },
                ArgType::Enum(_) => quote! {
                    match #arg_name.try_into() {
                        Ok(val) => val,
                        Err(_) => return Err(_wayrs_client::object::BadMessage),
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
                if __event.args.len() != #args_len {
                    return Err(_wayrs_client::object::BadMessage);
                }
                let mut __args = __event.args.drain(..);
                #( let Some(_wayrs_client::core::ArgValue::#arg_ty(#arg_names)) = __args.next() else { return Err(_wayrs_client::object::BadMessage) }; )*
                drop(__args);
                __pool.reuse_args(__event.args);
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
        let doc = gen_doc(en.description.as_ref(), None);
        let item_docs = en
            .items
            .iter()
            .map(|i| gen_doc(i.description.as_ref(), Some(i.since)));
        if en.is_bitfield {
            quote! {
                #doc
                #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
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
                impl ::std::ops::BitOrAssign for #name {
                    fn bitor_assign(&mut self, rhs: Self) {
                        self.0 |= rhs.0;
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
                    id: _wayrs_client::core::ObjectId::DISPLAY,
                    version: 1,
                };
            }
        }
    } else {
        quote!()
    };

    let event_exhaustiveness =
        (!FROZEN_IFACES.contains(&iface.name.as_str())).then(|| quote! { #[non_exhaustive] });

    quote! {
        #mod_doc
        #visibility mod #mod_name {
            #![allow(clippy::empty_docs)]

            use #wayrs_client_path as _wayrs_client;
            use _wayrs_client::object::Proxy;
            use _wayrs_client::EventCtx;

            #mod_doc
            #[doc = "See [`Event`] for the list of possible events."]
            #[derive(Clone, Copy)]
            pub struct #proxy_name {
                id: _wayrs_client::core::ObjectId,
                version: u32,
            }

            #extra_impl

            impl Proxy for #proxy_name {
                type Event = Event;

                const INTERFACE: &'static _wayrs_client::core::Interface
                    = &_wayrs_client::core::Interface {
                        name: _wayrs_client::cstr!(#raw_iface_name),
                        version: #iface_version,
                        events: &[ #(#events_desc,)* ],
                        requests: &[ #(#requests_desc,)* ],
                    };

                fn new(id: _wayrs_client::core::ObjectId, version: u32) -> Self {
                    Self { id, version }
                }

                fn parse_event(
                    mut __event: _wayrs_client::core::Message,
                    __self_version: u32,
                    __pool: &mut _wayrs_client::core::MessageBuffersPool,
                ) -> ::std::result::Result<Event, _wayrs_client::object::BadMessage> {
                    match __event.header.opcode {
                        #( #event_decoding )*
                        _ => Err(_wayrs_client::object::BadMessage),
                    }
                }

                fn id(&self) -> _wayrs_client::core::ObjectId {
                    self.id
                }

                fn version(&self) -> u32 {
                    self.version
                }
            }

            impl TryFrom<_wayrs_client::object::Object> for #proxy_name {
                type Error = _wayrs_client::object::WrongObject;

                fn try_from(object: _wayrs_client::object::Object) -> ::std::result::Result<Self, _wayrs_client::object::WrongObject> {
                    if object.interface == Self::INTERFACE {
                        Ok(Self {
                            id: object.id,
                            version: object.version,
                        })
                    } else {
                        Err(_wayrs_client::object::WrongObject)
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
                #[inline]
                fn eq(&self, other: &Self) -> bool {
                    self.id == other.id
                }
            }

            impl ::std::cmp::Eq for #proxy_name {}

            impl ::std::cmp::PartialEq<_wayrs_client::core::ObjectId> for #proxy_name {
                #[inline]
                fn eq(&self, other: &_wayrs_client::core::ObjectId) -> bool {
                    self.id == *other
                }
            }

            impl ::std::cmp::PartialEq<#proxy_name> for _wayrs_client::core::ObjectId {
                #[inline]
                fn eq(&self, other: &#proxy_name) -> bool {
                    *self == other.id
                }
            }

            impl ::std::cmp::PartialOrd for #proxy_name {
                #[inline]
                fn partial_cmp(&self, other: &Self) -> ::std::option::Option<::std::cmp::Ordering> {
                    ::std::option::Option::Some(::std::cmp::Ord::cmp(self, other))
                }
            }

            impl ::std::cmp::Ord for #proxy_name {
                #[inline]
                fn cmp(&self, other: &Self) -> ::std::cmp::Ordering {
                    self.id.cmp(&other.id)
                }
            }

            impl ::std::hash::Hash for #proxy_name {
                #[inline]
                fn hash<H>(&self, state: &mut H)
                    where H: ::std::hash::Hasher
                {
                    self.id.hash(state);
                }
            }

            impl ::std::borrow::Borrow<_wayrs_client::core::ObjectId> for #proxy_name {
                #[inline]
                fn borrow(&self) -> &_wayrs_client::core::ObjectId {
                    &self.id
                }
            }

            #( #event_args_structs )*
            #( #enums )*

            #[doc = "The event enum for [`"]
            #[doc = #proxy_name_str]
            #[doc = "`]"]
            #[derive(Debug)]
            #event_exhaustiveness
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
    where_: Option<TokenStream>,
    body: TokenStream,
) -> TokenStream {
    let name = make_ident(name);
    quote! {
        #attrs
        #[allow(clippy::too_many_arguments)]
        pub fn #name<#(#generics),*>(#(#args),*) -> #ret_ty #where_ {
            #body
        }
    }
}

fn gen_request_fn(opcode: u16, request: &Message) -> TokenStream {
    assert!(
        request
            .args
            .iter()
            .filter(|x| matches!(x.arg_type, ArgType::NewId { .. }))
            .count()
            <= 1,
        "{} has more than one new_id argument",
        request.name,
    );

    let new_id_interface = request.args.iter().find_map(|x| match &x.arg_type {
        ArgType::NewId { iface } => Some(iface.as_deref()),
        _ => None,
    });

    let mut fn_args = vec![
        quote!(self),
        quote!(conn: &mut _wayrs_client::Connection<D>),
    ];
    fn_args.extend(request.args.iter().flat_map(|arg| arg.as_request_fn_arg()));

    let msg_args = request.args.iter().map(|arg| {
        let arg_name = make_ident(&arg.name);
        let arg_ty = map_arg_to_argval(arg, false);
        match arg.arg_type {
            ArgType::NewId { iface: Some(_) } => {
                quote! { _wayrs_client::core::ArgValue::#arg_ty(Proxy::id(&new_object)) }
            }
            ArgType::NewId { iface: None } => {
                quote! { _wayrs_client::core::ArgValue::#arg_ty(
                    ::std::borrow::Cow::Borrowed(P::INTERFACE.name),
                    Proxy::version(&new_object),
                    Proxy::id(&new_object),
                ) }
            }
            ArgType::Object { allow_null, .. } => {
                if allow_null {
                    quote! { _wayrs_client::core::ArgValue::#arg_ty(#arg_name.as_ref().map(Proxy::id)) }
                } else {
                    quote! { _wayrs_client::core::ArgValue::#arg_ty(Proxy::id(&#arg_name)) }
                }
            }
            _ => quote! { _wayrs_client::core::ArgValue::#arg_ty(#arg_name.into()) },
        }
    });

    let send_message = quote! {
        let mut _args_vec = conn.alloc_msg_args();
        #( _args_vec.push(#msg_args); )*
        conn.send_request(
            Self::INTERFACE,
            _wayrs_client::core::Message {
                header: _wayrs_client::core::MessageHeader {
                    object_id: self.id,
                    size: 0,
                    opcode: #opcode,
                },
                args: _args_vec,
            }
        );
    };

    let doc = gen_doc(request.description.as_ref(), Some(request.since));

    match new_id_interface {
        None => gen_pub_fn(
            &doc,
            &request.name,
            &[quote!(D)],
            &fn_args,
            quote!(()),
            None,
            send_message,
        ),
        Some(None) => {
            let no_cb = gen_pub_fn(
                &doc,
                &request.name,
                &[quote!(P: Proxy), quote!(D)],
                &fn_args,
                quote!(P),
                None,
                quote! {
                    let new_object = conn.allocate_new_object::<P>(version);
                    #send_message
                    new_object
                },
            );
            fn_args.push(quote!(cb: impl FnMut(EventCtx<D, P>) + Send + 'static));
            let cb = gen_pub_fn(
                &doc,
                &format!("{}_with_cb", request.name),
                &[quote!(P: Proxy), quote!(D)],
                &fn_args,
                quote!(P),
                None,
                quote! {
                    let new_object = conn.allocate_new_object_with_cb(version, cb);
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
                None,
                quote! {
                    let new_object = conn.allocate_new_object::<#proxy_path>(self.version);
                    #send_message
                    new_object
                },
            );
            fn_args.push(quote!(cb: impl FnMut(EventCtx<D, #proxy_path>) + Send + 'static));
            let cb = gen_pub_fn(
                &doc,
                &format!("{}_with_cb", request.name),
                &[quote!(D)],
                &fn_args,
                proxy_path.clone(),
                None,
                quote! {
                    let new_object = conn.allocate_new_object_with_cb(self.version, cb);
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
    match &arg.arg_type {
        ArgType::Int => quote!(Int),
        ArgType::Uint | ArgType::Enum(_) => quote!(Uint),
        ArgType::Fixed => quote!(Fixed),
        ArgType::Object {
            allow_null: false, ..
        } => quote!(Object),
        ArgType::Object {
            allow_null: true, ..
        } => quote!(OptObject),
        ArgType::NewId { iface: None } => quote!(AnyNewId),
        ArgType::NewId { iface: Some(iface) } => {
            let proxy_name = make_proxy_path(iface);
            quote!(NewId(#proxy_name::INTERFACE))
        }
        ArgType::String { allow_null: false } => quote!(String),
        ArgType::String { allow_null: true } => quote!(OptString),
        ArgType::Array => quote!(Array),
        ArgType::Fd => quote!(Fd),
    }
}

fn map_arg_to_argval(arg: &Argument, is_event: bool) -> TokenStream {
    match &arg.arg_type {
        ArgType::Int => quote!(Int),
        ArgType::Uint | ArgType::Enum(_) => quote!(Uint),
        ArgType::Fixed => quote!(Fixed),
        ArgType::Object {
            allow_null: false, ..
        } => quote!(Object),
        ArgType::Object {
            allow_null: true, ..
        } => quote!(OptObject),
        ArgType::NewId { iface } if is_event => match iface.as_deref() {
            Some(_) => quote!(NewId),
            None => unimplemented!(),
        },
        ArgType::NewId { iface: None } => quote!(AnyNewId),
        ArgType::NewId { iface: Some(_) } => quote!(NewId),
        ArgType::String { allow_null: false } => quote!(String),
        ArgType::String { allow_null: true } => quote!(OptString),
        ArgType::Array => quote!(Array),
        ArgType::Fd => quote!(Fd),
    }
}

fn gen_doc(desc: Option<&Description>, since: Option<u32>) -> TokenStream {
    let since = since
        .map(|ver| format!("**Since version {ver}**.\n"))
        .map(|ver| quote!(#[doc = #ver]));

    let summary = desc
        .and_then(|d| d.summary.as_deref())
        .map(|s| format!("{}\n", s.trim()))
        .map(|s| quote!(#[doc = #s]));

    let text = desc
        .and_then(|d| d.text.as_deref())
        .into_iter()
        .flat_map(str::lines)
        .map(|s| format!("{}\n", s.trim()))
        .map(|s| quote!(#[doc = #s]));

    quote! {
        #summary
        #[doc = "\n"]
        #(#text)*
        #[doc = "\n"]
        #since
        #[doc = "\n"]
    }
}

trait ArgExt {
    fn as_request_fn_arg(&self) -> Option<TokenStream>;
    fn as_event_ty(&self) -> TokenStream;
}

impl ArgExt for Argument {
    fn as_request_fn_arg(&self) -> Option<TokenStream> {
        let arg_name = make_ident(&self.name);
        let retval = match &self.arg_type {
            ArgType::Int => quote!(#arg_name: i32),
            ArgType::Uint => quote!(#arg_name: u32),
            ArgType::Enum(enum_ty) => {
                if let Some((iface, name)) = enum_ty.split_once('.') {
                    let iface_name = syn::Ident::new(iface, Span::call_site());
                    let enum_name = make_pascal_case_ident(name);
                    quote!(#arg_name: super::#iface_name::#enum_name)
                } else {
                    let enum_name = make_pascal_case_ident(enum_ty);
                    quote!(#arg_name: #enum_name)
                }
            }
            ArgType::Fixed => quote!(#arg_name: _wayrs_client::core::Fixed),
            ArgType::Object {
                allow_null,
                iface: None,
            } => match allow_null {
                false => quote!(#arg_name: _wayrs_client::object::Object),
                true => quote!(#arg_name: ::std::option::Option<_wayrs_client::object::Object>),
            },
            ArgType::Object {
                allow_null,
                iface: Some(iface),
            } => {
                let proxy_path = make_proxy_path(iface);
                match allow_null {
                    false => quote!(#arg_name: #proxy_path),
                    true => quote!(#arg_name: ::std::option::Option<#proxy_path>),
                }
            }
            ArgType::NewId { iface: None } => quote!(version: u32),
            ArgType::NewId { iface: Some(_) } => return None,
            ArgType::String { allow_null } => match allow_null {
                false => quote!(#arg_name: ::std::ffi::CString),
                true => quote!(#arg_name: ::std::option::Option<::std::ffi::CString>),
            },
            ArgType::Array => quote!(#arg_name: ::std::vec::Vec<u8>),
            ArgType::Fd => quote!(#arg_name: ::std::os::fd::OwnedFd),
        };
        Some(retval)
    }

    fn as_event_ty(&self) -> TokenStream {
        match &self.arg_type {
            ArgType::Int => quote!(i32),
            ArgType::Uint => quote!(u32),
            ArgType::Enum(enum_ty) => {
                if let Some((iface, name)) = enum_ty.split_once('.') {
                    let iface_name = syn::Ident::new(iface, Span::call_site());
                    let enum_name = make_pascal_case_ident(name);
                    quote!(super::#iface_name::#enum_name)
                } else {
                    let enum_name = make_pascal_case_ident(enum_ty);
                    quote!(#enum_name)
                }
            }
            ArgType::Fixed => quote!(_wayrs_client::core::Fixed),
            ArgType::Object { allow_null, .. } => match allow_null {
                false => quote!(_wayrs_client::core::ObjectId),
                true => quote!(::std::option::Option<_wayrs_client::core::ObjectId>),
            },
            ArgType::NewId { iface: None } => quote!(_wayrs_client::object::Object),
            ArgType::NewId { iface: Some(iface) } => make_proxy_path(iface),
            ArgType::String { allow_null } => match allow_null {
                false => quote!(::std::ffi::CString),
                true => quote!(::std::option::Option<::std::ffi::CString>),
            },
            ArgType::Array => quote!(::std::vec::Vec<u8>),
            ArgType::Fd => quote!(::std::os::fd::OwnedFd),
        }
    }
}
