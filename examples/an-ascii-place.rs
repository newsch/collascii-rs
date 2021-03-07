//! An Ascii Place
//!
//! An example of (kind of mis)using the collascii Server trait.
//!
//! A server that lets each client place only a single character within a given time period.
//! Think Reddit's "The Place", but less scalable and in ascii.
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow;
use env_logger;
use log::{info, warn};
use structopt::StructOpt;

use collascii::network::*;
use collascii::Canvas;

#[derive(Debug, StructOpt)]
#[structopt(name = "an-ascii-place", author)]
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

    /// Wait time for client placement, in seconds
    #[structopt(long, default_value = "5")]
    wait: u64,
}

type ClientId = SocketAddr;

type Shared<T> = Arc<Mutex<T>>;

enum ThreadMessage {
    SetChar {
        id: ClientId,
        x: usize,
        y: usize,
        c: char,
    },
    Shutdown,
}

struct ClientConnection {
    wait: Duration,
    id: ClientId,
    input: BufReader<TcpStream>,
    output: TcpStream,
    last_write: Instant,

    canvas: Shared<Canvas>,
    sender: Sender<ThreadMessage>,
}

impl ClientConnection {
    fn run(&mut self) -> Result<(), anyhow::Error> {
        // init and send canvas
        self.init_connection()?;
        loop {
            // wait for setchars
            let (x, y, c) = self.check_for_update()?;

            // if after cooldown, accept/pass on
            let recv_time = Instant::now();
            if recv_time - self.last_write >= self.wait {
                self.last_write = recv_time; // reset timer
                self.sender.send(ThreadMessage::SetChar {
                    id: self.id,
                    x,
                    y,
                    c,
                })?;
            } else {
                // otherwise "reject" by overwriting sent char
                let c = self.canvas.lock().unwrap().get(x, y).to_owned();
                self.send_char_update(x, y, c.to_owned())?;
            }
        }
    }
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

struct ConnectionManager {
    wait: Duration,
    canvas: Shared<Canvas>,
    clients: Shared<HashMap<ClientId, TcpStream>>,
    listener: TcpListener,
    sender: Sender<ThreadMessage>,
}

impl ConnectionManager {
    fn run(&mut self) -> io::Result<()> {
        loop {
            let sender = self.sender.clone();
            let (stream, addr) = match self.listener.accept() {
                Ok(c) => c,
                Err(e) => {
                    warn!("Error accepting client: {}", e);
                    continue;
                }
            };
            let id = addr;
            info!("Client connected: {}", id);

            {
                let mut clients = self.clients.lock().unwrap();
                match clients.get(&id) {
                    Some(_) => panic!("Reference for client {} already exists!", id),
                    None => {
                        clients.insert(id, stream.try_clone().unwrap());
                    }
                }
            }

            let mut client = ClientConnection {
                wait: self.wait,
                id,
                output: stream.try_clone().unwrap(),
                input: BufReader::new(stream),
                last_write: Instant::now() - self.wait,
                canvas: self.canvas.clone(),
                sender,
            };

            thread::spawn(move || {
                // spawn client thread
                match client.run() {
                    Ok(()) => info!("Client {} quit", client.id),
                    Err(e) => info!("Client {} exited with error: {}", client.id, e),
                }
            });
        }
    }
}

struct CanvasKeeper {
    canvas: Shared<Canvas>,
    clients: Shared<HashMap<ClientId, TcpStream>>,
    receiver: Receiver<ThreadMessage>,
}

impl CanvasKeeper {
    fn run(&mut self) -> anyhow::Result<()> {
        loop {
            match self.receiver.recv()? {
                ThreadMessage::Shutdown => break,
                ThreadMessage::SetChar { id, x, y, c } => {
                    let mut clients = self.clients.lock().unwrap();
                    let msg = Message::CharSet { x, y, c };

                    let mut canvas = self.canvas.lock().unwrap();
                    if !canvas.is_in(x, y) {
                        warn!("Invalid set coordinates from client {}: ({},{})", id, x, y);
                        continue;
                    }

                    canvas.set(x, y, c);

                    for (&addr, stream) in clients.iter_mut() {
                        if addr == id {
                            continue;
                        }
                        // TODO: this could be through Messenger trait...
                        stream.write_fmt(format_args!("{}", msg))?;
                    }
                }
            }
        }
        Ok(())
    }
}

struct Place {
    keeper: CanvasKeeper,
    keeper_connection: Sender<ThreadMessage>,
    manager: ConnectionManager,
}

impl Place {
    fn from_opt(o: Opt) -> io::Result<Self> {
        let Opt {
            host,
            port,
            width,
            height,
            wait,
        } = o;

        let wait = Duration::from_secs(wait);

        info!("Listening on {}:{}", host, port);
        let listener = TcpListener::bind((&host[..], port))?;
        let (sender, receiver) = channel();
        let clients = Arc::new(Mutex::new(HashMap::new()));

        let keeper_connection = sender.clone();

        let canvas = Canvas::new(width, height);
        let canvas = Arc::new(Mutex::new(canvas));

        Ok(Self {
            keeper: CanvasKeeper {
                canvas: canvas.clone(),
                clients: clients.clone(),
                receiver,
            },
            keeper_connection,
            manager: ConnectionManager {
                wait,
                canvas,
                clients,
                listener,
                sender,
            },
        })
    }

    fn run(self) -> anyhow::Result<()> {
        let Self {
            mut keeper,
            keeper_connection,
            mut manager,
        } = self;
        let keeper_thread = thread::spawn(move || keeper.run());
        match manager.run() {
            Err(e) => warn!("Connection Manager failed: {}", e),
            Ok(()) => (),
        }
        keeper_connection.send(ThreadMessage::Shutdown)?;
        keeper_thread.join().expect("Keeper thread panicked")?;
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    {
        // init logging
        let mut builder = env_logger::Builder::from_default_env();
        builder.filter(None, log::LevelFilter::Info);
        builder.init();
    }
    let opt = Opt::from_args();
    let place = Place::from_opt(opt)?;
    place.run()?;
    Ok(())
}
