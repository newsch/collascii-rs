//! Output the contents of a collascii server's canvas
use std::io::{self, stdout, Write};
use std::net::{self, TcpStream};

use anyhow::{Context, Result};
use structopt::StructOpt;

use collascii::{Canvas, network::{Client, TcpClient, DEFAULT_PORT, ProtocolError}};

/// On connection, returns the canvas and closes the connection.
pub struct Dumper(TcpClient);

impl Dumper {
    pub fn connect<A: net::ToSocketAddrs>(addr: A) -> io::Result<Self> {
        let stream = TcpStream::connect(addr)?;
        Ok(Self(TcpClient::new(stream)?))
    }

    pub fn run(&mut self) -> Result<Canvas, ProtocolError> {
        self.0.init_connection()
    }
}

#[derive(Debug, StructOpt)]
#[structopt(
    name = "cl_dump",
    about = "Grab the current canvas from a collascii server",
    author
)]
struct Opt {
    /// IP/hostname to connect to
    #[structopt(default_value = "127.0.0.1")]
    host: String,

    /// Port to connect to
    #[structopt(default_value = DEFAULT_PORT)]
    port: u16,
}

fn main() -> Result<()> {
    let opt = Opt::from_args();

    let mut dumper = Dumper::connect((&opt.host[..], opt.port))
        .with_context(|| format!("Couldn't connect to tcp://{}:{}/", opt.host, opt.port))?;
    let canvas = dumper.run().ok().unwrap();
    stdout().write_all(canvas.as_str().as_bytes())?;
    Ok(())
}
