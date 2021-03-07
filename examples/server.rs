use std::fmt;
use std::io::{self, prelude::*};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::{collections::HashMap, io::BufReader};

use anyhow;
use env_logger;
use log::{debug, info, warn};
use structopt::StructOpt;

use collascii::network::{Message, DEFAULT_PORT};
use collascii::{
    canvas::Canvas,
    network::{ProtocolError, Server},
};

#[derive(Debug, StructOpt)]
#[structopt(
    name = "collascii-server",
    about = "A server for collascii, written in Rust",
    author
)]
struct Opt {
    /// Width of canvas
    #[structopt(short, long, default_value = "80")]
    width: usize,

    /// Height of canvas
    #[structopt(short, long, default_value = "24")]
    height: usize,

    /// Port to listen on
    #[structopt(short, long, default_value = DEFAULT_PORT)]
    port: u16,

    /// IP/hostname to listen on
    #[structopt(long, default_value = "127.0.0.1")]
    host: String,
}

fn main() -> anyhow::Result<()> {
    {
        // init logging
        let mut builder = env_logger::Builder::from_default_env();
        builder.filter(None, log::LevelFilter::Info);
        builder.init();
    }

    let opt = Opt::from_args();

    let mut canvas = Canvas::new(opt.width, opt.height);
    info!("Initial canvas size {}x{}", canvas.width(), canvas.height());

    canvas.insert("foobar");

    let canvas = Arc::new(Mutex::new(canvas));
    let clients = Arc::new(Mutex::new(Clients::new()));

    let listener = TcpListener::bind((opt.host.as_ref(), opt.port))?;

    info!("Listening at {}", listener.local_addr().unwrap());

    // accept connections and process them in parallel
    loop {
        let (stream, addr) = listener.accept().unwrap();
        let uid = clients.lock().unwrap().add(stream.try_clone().unwrap());
        info!("New client {} ({})", uid, addr);

        let handler = ClientConnection::new(uid, stream, &canvas, &clients);

        thread::spawn(move || match handler.run() {
            Ok(()) => info!("Client {} left", uid),
            Err(e) => warn!("Client {} disconnected: {}", uid, e),
        });
    }
}

/// A managed a socket connection to the server.
struct ClientConnection {
    uid: ClientUid,
    input: BufReader<TcpStream>,
    output: TcpStream,
    canvas: Arc<Mutex<Canvas>>,
    clients: Arc<Mutex<Clients>>,
}

impl Write for ClientConnection {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.output.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

impl Read for ClientConnection {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.input.read(buf)
    }
}

impl BufRead for ClientConnection {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.input.fill_buf()
    }
    fn consume(&mut self, amt: usize) {
        self.input.consume(amt)
    }
}

impl Server for ClientConnection {
    fn get_canvas(&self) -> Canvas {
        self.canvas.lock().unwrap().clone()
    }
}

impl ClientConnection {
    fn new(
        uid: ClientUid,
        stream: TcpStream,
        canvas: &Arc<Mutex<Canvas>>,
        clients: &Arc<Mutex<Clients>>,
    ) -> Self {
        let output = stream.try_clone().unwrap();
        let input = BufReader::new(stream);

        let canvas = canvas.clone();
        let clients = clients.clone();

        Self {
            uid,
            input,
            output,
            canvas,
            clients,
        }
    }

    /// Run the client connection to completion
    fn run(mut self) -> Result<(), ProtocolError> {
        self.init_connection()?;
        loop {
            match self.check_for_update() {
                Ok((x, y, c)) => {
                    let mut canvas = self.canvas.lock().unwrap();
                    if canvas.is_in(x, y) {
                        canvas.set(x, y, c);
                        debug!("Set {:?} to {:?} on local canvas", (x, y), c);
                    } else {
                        warn!(
                            "Position {:?} out of bounds for canvas of size {:?}",
                            (x, y),
                            (canvas.width(), canvas.height())
                        );
                        continue;
                    }

                    let msg = Message::CharSet { x, y, c };
                    let mut clients = self.clients.lock().unwrap();
                    clients.send(self.uid, format_args!("{}", msg))?;
                    debug!("Forwarded {:?} to other clients", msg);
                }
                Err(e) => {
                    self.clients.lock().unwrap().remove(self.uid);

                    return match e {
                        ProtocolError::Quit => Ok(()),
                        e => Err(e),
                    };
                }
            }
        }
    }
}

/// Unique identifier of a client
type ClientUid = u8;

/// Queue of connected network clients
struct Clients {
    list: HashMap<ClientUid, TcpStream>,
}

impl Clients {
    pub fn new() -> Self {
        Clients {
            list: HashMap::new(),
        }
    }

    /// Send a message to all clients but one (usually the sender)
    pub fn send(&mut self, client: ClientUid, msg: fmt::Arguments) -> io::Result<()> {
        for (&uid, stream) in self.list.iter_mut() {
            if uid == client {
                continue;
            }
            stream.write_fmt(msg)?
        }
        Ok(())
    }

    /// Add a client to the queue, returning the uid
    pub fn add(&mut self, client: TcpStream) -> ClientUid {
        let uid = self.get_new_uid();
        if self.list.insert(uid, client).is_some() {
            panic!("Uid should not exist in map!")
        }
        return uid;
    }

    /// Remove a client from the queue
    pub fn remove(&mut self, client: ClientUid) -> Option<TcpStream> {
        self.list.remove(&client)
    }

    /// Get a new uid for a client
    ///
    /// Warning: this will panic if the max uid is the maximum u8.
    fn get_new_uid(&self) -> ClientUid {
        match self.list.keys().max() {
            None => 1,
            Some(max_uid) => max_uid + 1,
        }
    }
}
