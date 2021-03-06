use std::io::{self, stdout, BufRead, BufReader, Read, Write};
use std::net::{self, TcpStream};

use anyhow::{Context, Result};
use structopt::StructOpt;

use collascii::{
    network::{Client, ProtocolError, DEFAULT_PORT},
    Canvas,
};

/// On connection, returns the canvas and closes the connection.
pub struct Dumper {
    input: BufReader<TcpStream>,
    output: TcpStream,
}

impl Dumper {
    pub fn connect<A: net::ToSocketAddrs>(addr: A) -> io::Result<Self> {
        let stream = TcpStream::connect(addr)?;
        let output = stream.try_clone()?;
        let input = BufReader::new(stream);
        Ok(Self { input, output })
    }

    pub fn run(&mut self) -> Result<Canvas, ProtocolError> {
        self.init_connection()
    }
}

impl Write for Dumper {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.output.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

impl Read for Dumper {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.input.read(buf)
    }
}

impl BufRead for Dumper {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.input.fill_buf()
    }
    fn consume(&mut self, amt: usize) {
        self.input.consume(amt)
    }
}

impl Client for Dumper {}

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
