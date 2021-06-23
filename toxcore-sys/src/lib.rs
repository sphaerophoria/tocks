#![allow(bad_style)]

//! Rust bindings for [toxcore](https://github.com/toktok/c-toxcore)

extern crate libsodium_sys;
extern crate audiopus_sys;
extern crate vpx_sys;

include!(concat!(env!("OUT_DIR"), "/toxcore.rs"));
