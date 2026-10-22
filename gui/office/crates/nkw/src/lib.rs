//! nkw: the world compositor (Smithay + glow). See docs/plano-mundo.md.
//!
//! The pure parts ([`world`], [`render`], [`scene`], [`script`], [`cli`],
//! [`keys`], [`stats`], [`shot`], [`router`]) are unit tested; the Smithay
//! side ([`state`], [`handlers`], [`input`], [`frame`], [`app`]) is tested
//! end to end in `tests/` with the headless backend.
//!
//! The plugin seam ([`ext`], contract S7): the hub calls the plugins
//! [`wm`], [`objects`], [`cats`] and [`overlay`] at fixed points, and the
//! [`router`] for every key.

pub mod app;
pub mod backend;
pub mod cats;
pub mod cli;
pub mod demo;
pub mod ext;
pub mod frame;
pub mod handlers;
pub mod input;
pub mod keys;
pub mod objects;
pub mod overlay;
pub mod render;
pub mod router;
pub mod scene;
pub mod script;
pub mod shot;
pub mod state;
pub mod stats;
pub mod wm;
pub mod world;

/// Entry point of the binary; returns the process exit code.
pub fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let env_reduced = std::env::var("NKW_REDUCED_MOTION").as_deref() == Ok("1");
    let cli = match cli::parse(&args, env_reduced) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return cli::exit::USAGE;
        }
    };
    if cli.help {
        println!("{}", cli::USAGE);
        return cli::exit::OK;
    }
    init_logging();
    let script = match &cli.script {
        None => None,
        Some(src) => {
            let text = match src {
                cli::ScriptSrc::Stdin => std::io::read_to_string(std::io::stdin()),
                cli::ScriptSrc::File(p) => std::fs::read_to_string(p),
            };
            let text = match text {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("nkw: script: {e}");
                    return cli::exit::USAGE;
                }
            };
            match script::parse(&text) {
                Ok(l) => Some(l),
                Err(e) => {
                    eprintln!("nkw: {e}");
                    return cli::exit::USAGE;
                }
            }
        }
    };
    match app::run(cli, script) {
        Ok(code) => code,
        Err((code, msg)) => {
            eprintln!("nkw: {msg}");
            code
        }
    }
}

fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_env("NKW_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}
