//! A quick example of a collascii-like curses interface.
//!
//! TODO: print debug messages to bottom of screen
//! TODO: connect to canvas instance
//! TODO: connect to server

extern crate pancurses;

fn main() {
    let window = pancurses::initscr();

    // CURSES CONFIG
    // try commenting these out to play around with different settings
    pancurses::nonl(); // don't convert \r to \n
    pancurses::cbreak(); // get characters immediately, don't wait for linebreaks
    pancurses::noecho(); // don't print input characters directly to the screen
    window.keypad(true); // interpret arrow keys and numpad as new distinct values, rather than send a sequence of control codes

    use pancurses::Input::{Character, KeyDown, KeyLeft, KeyRight, KeyUp};

    // read input characters until stopped
    loop {
        let (y, x) = window.get_cur_yx();
        // we can safely unwrap b/c window is not in nodelay mode
        let c = window.getch().unwrap();
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
                window.mv(y + ry, x + rx);
            }
            // print char to screen
            Character(c) => {
                // addch advances the cursor, there doesn't seem to be an option
                // to set a character without moving the cursor...
                // TODO: look into this more
                window.addch(c);
                window.mv(y, x);
            }
            // ignore everything else
            _ => {
                eprint!("Input character: {:?}", c);
                if let Character(ch) = c {
                    if let Some(name) = pancurses::keyname(ch as i32) {
                        eprint!(" ({})", name);
                    }
                }
                eprint!("\n");
            }
        }
    }
}
