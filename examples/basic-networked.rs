//! A quick example of a collascii-like curses interface.
//!
//! TODO: print debug messages to bottom of screen

extern crate env_logger;
extern crate log;
extern crate pancurses;

use collascii::canvas::Canvas;
use collascii::network::{Message, Version};

use std::cmp::{max, min};
use std::io::{self, BufRead, BufReader, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::time::Duration;

use log::{debug, info, log_enabled, trace};

const PROTOCOL_VERSION: Version = Version::new(1, 0);
const HOST: &str = "localhost";
const PORT: u16 = 5000;

fn main() {
    env_logger::init();
    debug!("Starting");

    // Net init
    let mut stream = TcpStream::connect((HOST, PORT))
        .expect(&format!("Couldn't connect to <{}:{}>", HOST, PORT));
    stream.set_read_timeout(Some(Duration::new(0, 1))).unwrap(); // don't block reads
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut canvas = {
        // send version request
        stream
            .write_fmt(format_args!(
                "{}",
                Message::VersionReq {
                    v: PROTOCOL_VERSION
                }
            ))
            .unwrap();
        // look for acknowledgment
        let m = Message::from_reader(&mut reader)
            .expect("Couldn't parse protocol version acknowledgment");
        if !matches!(m, Message::VersionAck) {
            panic!(
                "Got a message that wasn't a version acknowledgment: {:?}",
                m
            );
        }
        // get canvas
        let m = Message::from_reader(&mut reader).expect("Couldn't parse canvas data");

        match m {
            Message::CanvasSend { c: canvas } => canvas,
            _ => panic!("Got a message that wasn't a canvas update: {:?}", m),
        }
    };

    let window = pancurses::initscr();

    // CURSES CONFIG
    // try commenting these out to play around with different settings
    pancurses::nonl(); // don't convert \r to \n
    pancurses::cbreak(); // get characters immediately, don't wait for linebreaks
    pancurses::noecho(); // don't print input characters directly to the screen
    window.keypad(true); // interpret arrow keys and numpad as new distinct values, rather than send a sequence of control codes
    window.nodelay(true); // make wgetch non-blocking

    // draw canvas bounds
    for &(x, y) in [
        (canvas.width() - 1, 0),
        (canvas.width() - 1, canvas.height() - 1),
        (0, canvas.height() - 1),
    ]
    .iter()
    {
        canvas.set(x, y, 'X');
    }

    draw_canvas(&canvas, &window);
    window.mv(0, 0); // move to valid position at start

    // read input characters until stopped
    let mut peek_buffer: [u8; 1] = [0];
    loop {
        if let Some(c) = window.getch() {
            handle_key(c, &window, &mut canvas, &mut stream);
        }
        // TODO: switch to from_reader once better error handling done
        match stream.peek(&mut peek_buffer) {
            Err(e) => match e.kind() {
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => (),
                _ => panic!("Error reading from server: {}", e),
            },
            Ok(0) => break, // EOF
            Ok(_) => {
                let m = Message::from_reader(&mut reader);
                match m {
                    Ok(Message::SetChar { x, y, c }) => {
                        window.addch(c);
                        window.mv(y as i32, x as i32);
                        // update canvas
                        canvas.set(x as usize, y as usize, c);
                        debug!("Network update at {:?}", (x, y));
                    }
                    Ok(_) => panic!("Received unexpected message: {:?}", m),
                    Err(e) => panic!("Error reading from server: {:?}", e),
                }
            }
        }
    }
}

fn draw_canvas(c: &Canvas, window: &pancurses::Window) {
    let (win_height, win_width) = window.get_max_yx();
    let max_x = min(c.width(), win_width as usize + 1);
    let max_y = min(c.height(), win_height as usize + 1);
    for x in 0..max_x {
        for y in 0..max_y {
            window.mvaddch(y as i32, x as i32, *c.get(x, y));
        }
    }
}

fn handle_key(
    c: pancurses::Input,
    window: &pancurses::Window,
    canvas: &mut Canvas,
    server_write: &mut dyn Write,
) {
    use pancurses::Input::{Character, KeyDown, KeyLeft, KeyRight, KeyUp};

    let (y, x) = window.get_cur_yx();

    // log key inputs
    if log_enabled!(log::Level::Debug) {
        let mut msg = format!("Input: {:?}", c);
        if let Character(ch) = c {
            if let Some(name) = pancurses::keyname(ch as i32) {
                msg.push_str(&format!(" ({})", name));
            }
        }
        debug!("{}", msg);
    }

    match c {
        // move the cursor with arrow keys
        KeyRight | KeyLeft | KeyUp | KeyDown => {
            let (ry, rx) = match c {
                KeyLeft => (0, -1),
                KeyRight => (0, 1),
                KeyUp => (-1, 0),
                KeyDown => (1, 0),
                _ => unimplemented!(),
            };
            let (new_y, new_x) = (y + ry, x + rx);
            // fix pos if illegal
            let (max_y, max_x) = window.get_max_yx();
            let new_y = max(0, min(new_y, min(canvas.height() as i32 - 1, max_y)));
            let new_x = max(0, min(new_x, min(canvas.width() as i32 - 1, max_x)));
            window.mv(new_y, new_x);
        }
        // print char to screen
        Character(c) => {
            // update window
            // addch advances the cursor, there doesn't seem to be an option
            // to set a character without moving the cursor...
            // TODO: look into this more
            window.addch(c);
            window.mv(y, x);
            // update canvas
            canvas.set(x as usize, y as usize, c);
            // update server
            let msg = Message::SetChar {
                y: y as usize,
                x: x as usize,
                c,
            };
            server_write
                .write_fmt(format_args!("{}", msg))
                .expect("Error writing to server");
            debug!("Canvas updated at {:?}", (x, y));
        }
        // ignore everything else
        _ => (),
    }
}
