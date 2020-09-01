use std::collections::HashMap;
use std::fmt;
use std::io::{self, prelude::*, BufReader};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow;
use env_logger;
use log::{debug, info, warn};
use structopt::StructOpt;

use collascii::canvas::Canvas;
use collascii::network::{
    CollabId, Message, Messenger, ParseMessageError, ProtocolError, Server, DEFAULT_PORT,
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

        let read_stream = io::BufReader::new(stream.try_clone().unwrap());
        let write_stream = stream;

        thread::spawn(move || {
            let mut handler = ClientHandler {
                uid,
                write_stream,
                read_stream,
                canvas,
                clients,
                wants_pos_updates: false,
            };

            use ProtocolError::*;
            match handler.run() {
                Ok(()) => info!("Client {} left", uid),
                Err(Io(e)) => warn!("Client {} disconnected: {}", uid, e),
                Err(e) => warn!("Client {} disconnected: {}", uid, e),
            }
            handler.clients.lock().unwrap().remove(uid);
            let res = handler.write_stream.shutdown(Shutdown::Both);
            debug!("Closed stream: {:?}", res);
        });
    }
}

struct ClientHandler {
    uid: CollabId,
    write_stream: TcpStream,
    read_stream: BufReader<TcpStream>,
    canvas: Arc<Mutex<Canvas>>,
    clients: Arc<Mutex<Clients>>,
    wants_pos_updates: bool,
}

impl ClientHandler {
    fn run(&mut self) -> Result<(), ProtocolError> {
        use ProtocolError::*;

        self.init_connection()?;
        loop {
            match self.check_for_update() {
                Ok(()) => continue,
                Err(Quit) => break Ok(()),
                e => break e,
            }
        }
    }
}

impl Messenger for ClientHandler {
    fn send_msg(&mut self, msg: Message) -> Result<(), io::Error> {
        self.write_stream.write_fmt(format_args!("{}", msg))
    }
    fn get_msg(&mut self) -> Result<Message, ParseMessageError> {
        Message::from_reader(&mut self.read_stream)
    }
}

impl Server for ClientHandler {
    fn get_canvas(&self) -> Canvas {
        self.canvas.lock().unwrap().clone()
    }

    fn on_char_update(&mut self, x: usize, y: usize, c: char) {
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
            return;
        }

        let msg = Message::CharSet { x, y, c };

        let mut clients = self.clients.lock().unwrap();
        clients.send(self.uid, format_args!("{}", msg)).unwrap();
        debug!("Forwarded {:?} to other clients", msg);
    }

    fn on_pos_update(&mut self, x: usize, y: usize) {
        // TODO: check bounds of pos update
        let msg = Message::CollabPosSet { x, y, id: self.uid };
        self.clients
            .lock()
            .unwrap()
            .update_pos(self.uid, x, y)
            .expect("Error sending pos update"); // TODO: better error handling here; this should poison the mutex if sending to any client is an error

        // TODO: if this is the first time, set wants_pos_updates and send all known pos
        if !self.wants_pos_updates {
            let mut clients = self.clients.lock().unwrap();
            self.wants_pos_updates = true;
            let self_id = self.uid;
            clients.list.get_mut(&self.uid).unwrap().wants_pos_updates = true;
            for (id, c) in clients.list.iter().filter(|(id, _)| **id != self_id) {
                let id = *id;
                let (x, y) = c.pos;
                self.write_stream
                    .write_fmt(format_args!("{}", Message::CollabPosSet { id, x, y }))
                    .unwrap(); // TODO: error handling
            }
        }
        debug!("Forwarded {:?} to other clients", msg);
    }
}

struct ClientInfo {
    stream: TcpStream,
    pos: (usize, usize),
    wants_pos_updates: bool,
}

impl ClientInfo {
    fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            pos: (0, 0),
            wants_pos_updates: false,
        }
    }
}

/// Queue of connected network clients
struct Clients {
    list: HashMap<CollabId, ClientInfo>,
}

impl Clients {
    pub fn new() -> Self {
        Clients {
            list: HashMap::new(),
        }
    }

    /// Send a message to all clients
    pub fn broadcast(&mut self, msg: fmt::Arguments) -> io::Result<()> {
        for (_uid, ClientInfo { stream, .. }) in self.list.iter_mut() {
            stream.write_fmt(msg)?
        }
        Ok(())
    }

    /// Send a message to all clients but one (usually the sender)
    pub fn send(&mut self, client: CollabId, msg: fmt::Arguments) -> io::Result<()> {
        for (&uid, ClientInfo { stream, .. }) in self.list.iter_mut() {
            if uid == client {
                continue;
            }
            stream.write_fmt(msg)?
        }
        Ok(())
    }

    pub fn filter_send<P>(
        &mut self,
        mut predicate: P,
        ignore: CollabId,
        msg: fmt::Arguments,
    ) -> io::Result<()>
    where
        P: FnMut(&ClientInfo) -> bool,
    {
        for (_, ClientInfo { stream, .. }) in self
            .list
            .iter_mut()
            .filter(|(id, _)| **id != ignore)
            .filter(|(_, c)| predicate(c))
        {
            stream.write_fmt(msg)?
        }
        Ok(())
    }

    /// Add a client to the queue, returning the uid
    pub fn add(&mut self, stream: TcpStream) -> CollabId {
        let uid = self.get_new_uid();
        let info = ClientInfo::new(stream);
        if self.list.insert(uid, info).is_some() {
            panic!("Uid should not exist in map!")
        }
        return uid;
    }

    /// Remove a client from the queue
    pub fn remove(&mut self, client: CollabId) -> Option<TcpStream> {
        self.list.remove(&client).map(|c| c.stream)
    }

    /// Get a new uid for a client
    ///
    /// Warning: this will panic if the max uid is the maximum u8.
    fn get_new_uid(&self) -> CollabId {
        match self.list.keys().max() {
            None => 1,
            Some(max_uid) => max_uid + 1,
        }
    }

    fn update_pos(&mut self, id: CollabId, x: usize, y: usize) -> io::Result<()> {
        self.list.get_mut(&id).unwrap().pos = (x, y);
        let msg = Message::CollabPosSet { x, y, id };
        self.filter_send(|c| c.wants_pos_updates, id, format_args!("{}", msg))
    }
}
