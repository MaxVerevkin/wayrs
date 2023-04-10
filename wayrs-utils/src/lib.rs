//! A collection of utils and abstractions for `wayrs-client`

#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "seats")]
#[cfg_attr(docsrs, doc(cfg(feature = "seats")))]
pub mod seats;

#[cfg(feature = "shm_alloc")]
#[cfg_attr(docsrs, doc(cfg(feature = "shm_alloc")))]
pub mod shm_alloc;

#[cfg(feature = "cursor")]
#[cfg_attr(docsrs, doc(cfg(feature = "cursor")))]
pub mod cursor;

#[cfg(feature = "keyboard")]
#[cfg_attr(docsrs, doc(cfg(feature = "keyboard")))]
pub mod keyboard;
