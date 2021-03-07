//! Copy the contents of a file to a collascii server's canvas
use std::fs;
use std::io::{self, stdin, BufRead, BufReader, Read, Write};
use std::net::{self, TcpStream};

use anyhow::{bail, Context, Result};
use structopt::StructOpt;

use collascii::network::{Client, ProtocolError, DEFAULT_PORT};
use collascii::Canvas;

/// On connection, returns the canvas and closes the connection.
pub struct Loader {
    input: BufReader<TcpStream>,
    output: TcpStream,
}

impl Loader {
    pub fn connect<A: net::ToSocketAddrs>(addr: A) -> io::Result<Self> {
        let stream = TcpStream::connect(addr)?;
        let output = stream.try_clone()?;
        let input = BufReader::new(stream);
        Ok(Self { input, output })
    }

    pub fn send_canvas(&mut self, c: &Canvas) -> Result<(), ProtocolError> {
        for i in 0..(c.height() * c.width()) {
            let val = *c.geti(i);
            let (x, y) = c.i_to_xy(i);
            self.send_char_update(x, y, val)?;
        }
        Ok(())
    }
}

impl Write for Loader {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.output.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

impl Read for Loader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.input.read(buf)
    }
}

impl BufRead for Loader {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.input.fill_buf()
    }
    fn consume(&mut self, amt: usize) {
        self.input.consume(amt)
    }
}

impl Client for Loader {}

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

    let existing = loader.init_connection()?;
    if existing.width() < replacement.width() || existing.height() < replacement.height() {
        bail!(
            "Server canvas is smaller than input: {}x{} < {}x{}",
            existing.width(),
            existing.height(),
            replacement.width(),
            replacement.height()
        )
    }
    loader.send_canvas(&replacement)?;
    Ok(())
}
