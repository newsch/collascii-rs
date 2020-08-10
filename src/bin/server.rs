use std::collections::HashMap;
use std::fmt;
use std::io::{self, prelude::*};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow;
use env_logger;
use log::{debug, info, warn};
use structopt::StructOpt;

use collascii::canvas::Canvas;
use collascii::network::{Message, Version, DEFAULT_PORT};

const PROTOCOL_VERSION: Version = Version::new(1, 0);

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
    println!("{:?}", opt);

    let mut canvas = Canvas::new(opt.width, opt.height);
    info!("Initial canvas size {}x{}", canvas.width(), canvas.height());

    canvas.insert("foobar");

    let canvas = Arc::new(Mutex::new(canvas));
    let clients = Arc::new(Mutex::new(Clients::new()));

    let listener = TcpListener::bind((&opt.host[..], opt.port))?;

    info!("Listening at {}", listener.local_addr().unwrap());

    // accept connections and process them in parallel
    loop {
        let (stream, addr) = listener.accept().unwrap();
        let uid = clients.lock().unwrap().add(stream.try_clone().unwrap());
        info!("New client {} ({})", uid, addr);
        let canvas = canvas.clone();
        let clients = clients.clone();

        thread::spawn(move || {
            match handle_stream(uid, stream, &canvas, &clients) {
                Ok(()) => info!("Client {} left", uid),
                Err(e) => warn!("Client {} disconnected: {}", uid, e),
            }
            clients.lock().unwrap().remove(uid);
        });
    }
}

/// Manage a socket connection to the server.
///
/// Returns when the connection ends.
fn handle_stream(
    uid: ClientUid,
    mut stream: TcpStream,
    canvas: &Mutex<Canvas>,
    clients: &Mutex<Clients>,
) -> io::Result<()> {
    // for each client:
    // - check protocol version
    // - send canvas
    // - on message received, interpret, modify, and forward

    let mut read_stream = io::BufReader::new(stream.try_clone().unwrap());

    // protocol version negotiation
    let _version = {
        let msg = Message::from_reader(&mut read_stream)?;
        if let Message::VersionReq { v } = msg {
            // set version
            if v != PROTOCOL_VERSION {
                stream.write_all(b"Unknown version\n")?;
                panic!("Unknown version");
            }
            stream.write_fmt(format_args!("{}", Message::VersionAck))?;
            v
        } else {
            panic!("Expected version statement");
        }
    };

    // send current canvas to new client
    {
        let c = canvas.lock().unwrap();
        let msg = Message::CanvasSet { c: c.clone() };
        stream.write_fmt(format_args!("{}", msg))?;
    }

    // main loop
    loop {
        let msg = Message::from_reader(&mut read_stream)?;
        debug!("Parsed message from input stream: {:?}", msg);
        use Message::*;
        match msg {
            Quit => {
                // stop and exit
                clients.lock().unwrap().remove(uid);
                return Ok(());
            }
            CharSet { y, x, c } => {
                // update canvas and broadcast to others
                {
                    let mut canvas = canvas.lock().unwrap();
                    if canvas.is_in(x, y) {
                        canvas.set(x, y, c);
                        debug!("Set {:?} to {:?} on local canvas", (x, y), c);
                    } else {
                        warn!(
                            "Position {:?} out of bounds for canvas of size {:?}",
                            (x, y),
                            (canvas.width(), canvas.height())
                        );
                    }
                }

                let mut clients = clients.lock().unwrap();
                clients.send(uid, format_args!("{}", msg))?;
                debug!("Forwarded {:?} to other clients", msg);
            }
            CanvasSet { c: _ } => {
                // swap canvas, broadcast to others
                unimplemented!()
            }
            m => panic!("Unexpected message: {:?}", m), // TODO: move this to a result
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

    /// Send a message to all clients
    pub fn broadcast(&mut self, msg: fmt::Arguments) -> io::Result<()> {
        for (_uid, stream) in self.list.iter_mut() {
            stream.write_fmt(msg)?
        }
        Ok(())
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
