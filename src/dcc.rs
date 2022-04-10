use std::fs::File;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::time::Duration;

use anyhow::Result;
use indicatif::ProgressBar;

pub struct Download {
    pub filename: String,
    pub size: usize,
    stream: TcpStream,
}

impl Download {
    pub fn new(filename: String, address: &str, port: u16, size: usize) -> Result<Self> {
        let socket_address = &(address, port).to_socket_addrs()?.next().unwrap();
        let stream = TcpStream::connect_timeout(socket_address, Duration::from_secs(15))?;

        Ok(Self {
            filename,
            size,
            stream,
        })
    }

    pub fn download(mut self, pb: Option<ProgressBar>) -> Result<()> {
        let mut file = File::create(&self.filename)?;

        let mut buf = [0; 4096];
        let mut progress: usize = 0;

        while progress < self.size {
            let count = self.stream.read(&mut buf)?;
            progress += count;
            file.write_all(&buf[..count])?;
            if let Some(ref p) = pb {
                p.set_position(progress as u64);
            }
        }

        self.stream.shutdown(Shutdown::Both)?;
        file.flush()?;

        Ok(())
    }
}
