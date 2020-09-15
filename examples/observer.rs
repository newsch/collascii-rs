//! Read-only view of a canvas
use std::io::{self, stdout, BufRead, BufReader, Read, Write};
use std::{
    cmp::{max, min},
    collections::HashMap,
    net::{self, TcpStream},
};

use anyhow::{Context, Result};
use structopt::StructOpt;
extern crate pancurses;

use collascii::{
    network::{Client, CollabId, Message, Messenger, ProtocolError, DEFAULT_PORT},
    Canvas,
};

struct Collaborator {
    pos: (usize, usize),
}

struct Observer {
    input: BufReader<TcpStream>,
    output: TcpStream,
    collabs: HashMap<CollabId, Collaborator>,
    window: pancurses::Window,
}

impl Observer {
    pub fn connect(opt: Opt) -> Result<Self, ProtocolError> {
        let addr = (opt.host, opt.port);
        let stream = TcpStream::connect(addr)?;
        let output = stream.try_clone()?;
        let input = BufReader::new(stream);
        let collabs = HashMap::new();

        let window = pancurses::initscr();
        window.clear();
        pancurses::nonl(); // don't convert \r to \n
        pancurses::cbreak(); // get characters immediately, don't wait for linebreaks
        pancurses::noecho(); // don't print input characters directly to the screen
        // window.keypad(true); // interpret arrow keys and numpad as new distinct values, rather than send a sequence of control codes
        window.nodelay(true); // make wgetch non-blocking
        // pancurses::use_default_colors();

        Ok(Self {
            input,
            output,
            collabs,
            window,
        })
    }

    pub fn run(&mut self) -> Result<(), ProtocolError> {
        let canvas = self.init_connection()?;
        self.draw_canvas(canvas);
        loop {
            self.window.refresh();
            self.check_for_update()?;
            // break Ok(());
        }
    }

    fn visible(&mut self, x: usize, y: usize) -> bool {
        let (max_y, max_x) = self.window.get_max_yx();
        (x as i32) < max_x && (y as i32) < max_y
    }

    fn draw_canvas(&mut self, c: Canvas) {
        let cols = min(self.window.get_max_x() as usize, c.width());
        let rows = min(self.window.get_max_y() as usize, c.height());
        for x in 0..cols {
            for y in 0..rows {
                self.window.mvaddch(y as i32, x as i32, *c.get(x, y));
            }
        }
    }
}

impl Messenger for Observer {
    fn send_msg(&mut self, msg: collascii::network::Message) -> Result<(), io::Error> {
        self.output.write_fmt(format_args!("{}", msg))
    }

    fn get_msg(
        &mut self,
    ) -> Result<collascii::network::Message, collascii::network::ParseMessageError> {
        Message::from_reader(&mut self.input)
    }
}

impl Client for Observer {
    fn on_char_update(&mut self, x: usize, y: usize, c: char) {
        if self.visible(x, y) {
            self.window.mvaddch(y as i32, x as i32, c);
        }
    }

    fn on_pos_update(&mut self, x: usize, y: usize, id: CollabId) {
        // update new position
        if self.visible(x, y) {
            self.window.mv(y as i32, x as i32);
            self.window
                .mvchgat(y as i32, x as i32, 1, pancurses::A_REVERSE, 0);
        }
        // reset old position
        let is_used: bool = self.collabs.iter().any(|(_, c)| c.pos == (x, y));
        if !is_used {
            self.window
                .mvchgat(y as i32, x as i32, 1, pancurses::A_NORMAL, 0);
        }
        // update map
        self.collabs.insert(id, Collaborator { pos: (x, y)});
    }
}

#[derive(Debug, StructOpt)]
#[structopt(
    name = "observer",
    about = "Read-only view of a canvas",
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

fn run() -> Result<()> {
    let opt = Opt::from_args();

    let mut observer = Observer::connect(opt)?;

    observer.run()?;

    Ok(())
}

fn main() -> Result<()> {
    let res = run();
    pancurses::endwin();
    res
    // match res {
    //     Ok(_) => (),
    //     Err(e) => eprintln!("{}", e),
    // }
}
