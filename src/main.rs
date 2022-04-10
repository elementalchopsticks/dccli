#![feature(backtrace, byte_slice_trim_ascii, once_cell)]

mod dcc;
mod irc;

use irc::{Irc, Status};

use std::backtrace::BacktraceStatus;
use std::lazy::SyncLazy;

use anyhow::{Context, Result};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};

#[derive(Parser, Debug)]
#[clap(version)]
pub struct Args {
    /// Bot to receive DCC from
    bot: String,
    /// XDCC pack number
    pack: usize,

    /// Add IRC channel to join (omit '#')
    #[clap(short, long, value_name = "CHAN")]
    channel: Vec<String>,
    /// Set IRC connection port
    #[clap(short, long, value_name = "PORT", default_value_t = 6667)]
    port: u16,
    /// Set IRC server address
    #[clap(short, long, value_name = "ADDR", default_value = "irc.rizon.net")]
    server: String,
    /// Show more information
    #[clap(short, long)]
    verbose: bool,
}

static ARGS: SyncLazy<Args> = SyncLazy::new(Args::parse);

fn run(args: &'static Args) -> Result<()> {
    let mut irc = Irc::connect(args).context("failed to connect to IRC server")?;

    let mut handles = Vec::new();

    let pb = if !args.verbose {
        let pb = ProgressBar::new_spinner();
        pb.set_style(ProgressStyle::default_spinner().template(&format!(
            "[{{elapsed_precise}}] Connecting to {}... {{spinner}}",
            args.server
        )));
        Some(pb)
    } else {
        None
    };

    loop {
        if let Some(p) = pb.as_ref() {
            p.tick();
        }
        match irc.handle_messages(pb.clone())? {
            Status::Ok => {}
            Status::ConnectionClosed => {
                if let Some(p) = pb {
                    p.finish_and_clear();
                }
                break;
            }
            Status::NewDcc(dl) => {
                let dlpb = if !args.verbose {
                    let pb = ProgressBar::new(dl.size as u64);
                    pb.set_style(ProgressStyle::default_spinner().template(&format!(
                        "Downloading \"{}\" [{{elapsed_precise}}] {{bar}} {{bytes}}/{{total_bytes}} ({{eta}})",
                        dl.filename
                    )).progress_chars("=>-"));
                    Some(pb)
                } else {
                    None
                };

                handles.push(std::thread::spawn(move || dl.download(dlpb)));
                irc.quit()?;
            }
        }
    }

    for handle in handles {
        handle.join().unwrap()?;
    }

    Ok(())
}

fn main() {
    let result = run(&ARGS);
    match result {
        Ok(_) => {}
        Err(error) => {
            eprintln!("dccli: {:#}", &error);
            if error.backtrace().status() == BacktraceStatus::Captured {
                eprint!("\nStack backtrace:\n{}", error.backtrace());
            }
            std::process::exit(1);
        }
    }
}
