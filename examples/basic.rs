//! A quick example of a collascii-like curses interface.
//!
//! TODO: print debug messages to bottom of screen

extern crate env_logger;
extern crate log;
extern crate pancurses;

use collascii::canvas::Canvas;

use log::{debug, log_enabled};
use std::cmp::{max, min};

fn main() {
    env_logger::init();
    debug!("Starting");
    let window = pancurses::initscr();

    // CURSES CONFIG
    // try commenting these out to play around with different settings
    pancurses::nonl(); // don't convert \r to \n
    pancurses::cbreak(); // get characters immediately, don't wait for linebreaks
    pancurses::noecho(); // don't print input characters directly to the screen
    window.keypad(true); // interpret arrow keys and numpad as new distinct values, rather than send a sequence of control codes

    use pancurses::Input::{Character, KeyDown, KeyLeft, KeyRight, KeyUp};

    // init canvas and draw to window
    let mut canvas = Canvas::new(80, 20);
    canvas.insert("Hello there");
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
    loop {
        let (y, x) = window.get_cur_yx();

        // we can safely unwrap b/c window is not in nodelay mode
        let c = window.getch().unwrap();

        // log key inputs
        if log_enabled!(log::Level::Debug) {
            let mut msg = format!("Input: {:?}", c);
            // eprint!();
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
                debug!("Canvas updated at {:?}", (x, y));
            }
            // ignore everything else
            _ => (),
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
