//! Copy the contents of a file to a collascii server's canvas
use std::fs;
use std::io::{stdin, Read};
use std::net::{self};

use anyhow::{bail, Context, Result};
use structopt::StructOpt;

use collascii::network::{Client, ProtocolError, TcpClient, DEFAULT_PORT};
use collascii::Canvas;

/// On connection, returns the canvas and closes the connection.
pub struct Loader(TcpClient, Canvas);

impl Loader {
    pub fn connect<A: net::ToSocketAddrs>(addr: A) -> Result<Self, ProtocolError> {
        let mut client = TcpClient::connect(addr)?;
        let canvas = client.init_connection()?;
        Ok(Self(client, canvas))
    }

    pub fn width(&self) -> usize {
        self.1.width()
    }

    pub fn height(&self) -> usize {
        self.1.height()
    }

    pub fn send_canvas(&mut self, c: &Canvas) -> Result<(), ProtocolError> {
        for i in 0..(c.height() * c.width()) {
            let val = *c.geti(i);
            let (x, y) = c.i_to_xy(i);
            self.0.send_char_update(x, y, val)?;
        }
        Ok(())
    }
}

#[derive(Debug, StructOpt)]
#[structopt(
    name = "cl_restore",
    about = "Replace the canvas on a collascii server",
    author
)]
struct Opt {
    /// IP/hostname to connect to
    #[structopt(default_value = "127.0.0.1")]
    host: String,

    /// Port to connect to
    #[structopt(default_value = DEFAULT_PORT)]
    port: u16,

    /// File to read from (defaults to stdin)
    #[structopt(long, short)]
    file: Option<String>,
}

fn main() -> Result<()> {
    let opt = Opt::from_args();

    let mut loader = Loader::connect((&opt.host[..], opt.port)).with_context(|| {
        format!(
            "Couldn't connect to server at tcp://{}:{}/",
            opt.host, opt.port
        )
    })?;

    let s = match opt.file {
        Some(path) => fs::read_to_string(path)?,
        None => {
            let mut s = String::new();
            stdin().read_to_string(&mut s)?;
            s
        }
    };

    let replacement = Canvas::from(s.as_str());

    if loader.width() < replacement.width() || loader.height() < replacement.height() {
        bail!(
            "Server canvas is smaller than input: {}x{} < {}x{}",
            loader.width(),
            loader.height(),
            replacement.width(),
            replacement.height()
        )
    }
    loader.send_canvas(&replacement)?;
    Ok(())
}
