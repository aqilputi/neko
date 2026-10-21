//! nkg — nekuroddo's native GUI: an office where the cats work with their
//! backs to us, one niri column per session, code running on their monitors.

// Bevy systems take long, typed queries and many params by design.
#![allow(clippy::type_complexity, clippy::too_many_arguments)]

mod app;
mod catpose;
mod cli;
mod core_io;
mod inbox;
mod keys;
mod model;
mod monitor;
mod nav;
mod raster;
mod scene;
mod strip;
#[cfg(not(feature = "client"))]
mod stub;

/// The client API: the real crate, or the local stub without `client`.
mod nkc {
    #[cfg(not(feature = "client"))]
    pub use crate::stub::*;
    #[cfg(feature = "client")]
    pub use nk_client::*;
}

fn main() {
    let env_reduced = std::env::var("NKG_REDUCED_MOTION").is_ok_and(|v| !v.is_empty() && v != "0");
    let cli = match cli::parse(std::env::args().skip(1), env_reduced) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    app::run(cli);
}
