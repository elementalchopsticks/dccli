use crate::dcc::Download;
use crate::Args;

use std::io::{ErrorKind, Read, Write};
use std::lazy::SyncLazy;
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::str::from_utf8;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Local;
use indicatif::{ProgressBar, ProgressStyle};
use regex::Regex;

pub enum Status {
    Ok,
    ConnectionClosed,
    NewDcc(Download),
}

pub struct Irc {
    args: &'static Args,
    stream: TcpStream,
    remainder: Vec<u8>,
}

const USERNAME: &str = "TestTest";

static PING_RE: SyncLazy<Regex> = SyncLazy::new(|| Regex::new(r#"^(?:\S+ )?PING (\S+)"#).unwrap());
static MODE_RE: SyncLazy<Regex> =
    SyncLazy::new(|| Regex::new(r#"^(?:\S+ )?MODE \S+ :\S+$"#).unwrap());
static DCC_RE: SyncLazy<Regex> =
    SyncLazy::new(|| Regex::new(r#"DCC SEND "?(.+)"? (\d+) (\d+) (\d+)"#).unwrap());

// TODO: for some reason this regex doesn't work
/*static DCC_RE: SyncLazy<Regex> = SyncLazy::new(|| {
    Regex::new(r#"^(?:\S+ )?PRIVMSG \S+ :DCC SEND "(.+)" (\d+) (\d+) (\d+)$"#).unwrap()
});*/

impl Irc {
    pub fn connect(args: &'static Args) -> Result<Self> {
        let socket_address = &(args.server.as_ref(), args.port)
            .to_socket_addrs()?
            .next()
            .unwrap();
        let mut stream = TcpStream::connect_timeout(socket_address, Duration::from_secs(15))?;
        stream.set_read_timeout(Some(Duration::from_millis(100)))?;
        stream.set_write_timeout(Some(Duration::from_secs(15)))?;

        send_message(
            &mut stream,
            &format!("USER {0} 0 * {0}", USERNAME),
            args.verbose,
        )?;
        send_message(&mut stream, &format!("NICK {}", USERNAME), args.verbose)?;

        let remainder = Vec::new();

        Ok(Self {
            args,
            stream,
            remainder,
        })
    }

    pub fn quit(&mut self) -> Result<()> {
        send_message(&mut self.stream, "QUIT", self.args.verbose)?;
        self.stream.shutdown(Shutdown::Both)?;
        Ok(())
    }

    pub fn handle_messages(&mut self, pb: Option<ProgressBar>) -> Result<Status> {
        let mut buf = [0; 1024];
        let read = match self.stream.read(&mut buf) {
            Ok(count) => {
                if count == 0 {
                    self.stream.shutdown(Shutdown::Both)?;
                    return Ok(Status::ConnectionClosed);
                }
                &buf[..count]
            }
            Err(error) => match error.kind() {
                ErrorKind::WouldBlock => return Ok(Status::Ok),
                _ => return Err(anyhow::Error::from(error)),
            },
        };

        let mut it = read.split(|elem| *elem == b'\n').peekable();
        while let Some(line) = it.next() {
            if it.peek().is_none() {
                self.remainder.append(&mut line.to_owned());
                continue;
            }

            let message = if !self.remainder.is_empty() {
                let full = [self.remainder.trim_ascii_start(), line.trim_ascii_end()].concat();
                self.remainder = Vec::new();
                String::from_utf8(full).unwrap()
            } else {
                from_utf8(line).unwrap().trim().to_owned()
            };

            if self.args.verbose {
                println!(
                    "\x1b[34m[{} IRC RECV]\x1b[0m {}",
                    Local::now().format("%F %T%.3f"),
                    message
                );
            }

            if let Some(dl) = Self::handle_message(self, &message, pb.clone())? {
                return Ok(Status::NewDcc(dl));
            };
        }

        Ok(Status::Ok)
    }

    fn handle_message(
        &mut self,
        message: &str,
        pb: Option<ProgressBar>,
    ) -> Result<Option<Download>> {
        // TODO: check for ERROR messages
        if PING_RE.is_match(message) {
            let caps = PING_RE.captures(message).unwrap();
            send_message(
                &mut self.stream,
                &format!("PONG {}", &caps[1]),
                self.args.verbose,
            )?;
        } else if MODE_RE.is_match(message) {
            for channel in self.args.channel.iter() {
                send_message(
                    &mut self.stream,
                    &format!("JOIN #{}", channel),
                    self.args.verbose,
                )?;
            }
            send_message(
                &mut self.stream,
                &format!("PRIVMSG {} :XDCC GET #{}", self.args.bot, self.args.pack),
                self.args.verbose,
            )?;
            if let Some(p) = pb {
                p.set_style(
                    ProgressStyle::default_spinner()
                        .template("[{elapsed_precise}] Waiting for dcc connection... {spinner}"),
                );
            }
        } else if DCC_RE.is_match(message) {
            // TODO: check origin of message and match with bot
            let caps = DCC_RE.captures(message).unwrap();
            let filename = caps[1].to_owned();
            let address = &caps[2];
            let port = caps[3].parse::<u16>().unwrap();
            let size = caps[4].parse::<usize>().unwrap();

            let dl = Download::new(filename, address, port, size)
                .context("failed to initialise dcc connection")?;

            return Ok(Some(dl));
        }

        Ok(None)
    }
}

fn send_message(stream: &mut TcpStream, message: &str, verbose: bool) -> Result<()> {
    if verbose {
        println!(
            "\x1b[32m[{} IRC SEND]\x1b[0m {}",
            Local::now().format("%F %T%.3f"),
            message
        );
    }
    stream.write_all(message.as_bytes())?;
    stream.write_all(&[b'\r', b'\n'])?;
    Ok(())
}
