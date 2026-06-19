//! The runtime catalog: static element/attribute arrays generated at build time
//! from the committed, extracted spec data (see `build.rs`).

include!(concat!(env!("OUT_DIR"), "/catalog.rs"));
