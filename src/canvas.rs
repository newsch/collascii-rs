use std::fmt;
use std::io::{self, Read};
use std::ops::{Index, IndexMut};
use std::vec::Vec;

#[derive(Debug, PartialEq, Clone)]
pub struct Canvas {
    width: usize,
    height: usize,
    rows: Vec<Vec<char>>,
}

impl Canvas {
    pub fn new(width: usize, height: usize) -> Self {
        let fill = ' '; // initial character to fill canvas with
        let mut rows = Vec::with_capacity(height as usize);
        for _ in 0..height {
            let mut v = Vec::with_capacity(width as usize);
            v.resize(width, fill);
            rows.push(v);
        }
        Canvas {
            width,
            height,
            rows,
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn get(&self, x: usize, y: usize) -> &char {
        debug_assert!(
            self.is_in(x, y),
            "Get index {:?} out of bounds for canvas of size {:?}",
            (x, y),
            (self.width, self.height)
        );
        &self.rows[y][x]
    }

    pub fn get_mut(&mut self, x: usize, y: usize) -> &mut char {
        debug_assert!(
            self.is_in(x, y),
            "Get index {:?} out of bounds for canvas of size {:?}",
            (x, y),
            (self.width, self.height)
        );
        &mut self.rows[y][x]
    }

    pub fn geti(&self, i: usize) -> &char {
        debug_assert!(
            self.is_in_i(i),
            "Get index {:?} out of bounds for canvas of size {:?}",
            i,
            (self.width, self.height)
        );
        let (x, y) = self.i_to_xy(i);
        self.get(x, y)
    }

    pub fn geti_mut(&mut self, i: usize) -> &mut char {
        debug_assert!(
            self.is_in_i(i),
            "Get index {:?} out of bounds for canvas of size {:?}",
            i,
            (self.width, self.height)
        );
        let (x, y) = self.i_to_xy(i);
        self.get_mut(x, y)
    }

    pub fn set(&mut self, x: usize, y: usize, val: char) {
        debug_assert!(
            self.is_in(x, y),
            "Set index {:?} out of bounds for canvas of size {:?}",
            (x, y),
            (self.width, self.height)
        );
        self.rows[y][x] = val;
    }

    pub fn seti(&mut self, i: usize, val: char) {
        debug_assert!(
            self.is_in_i(i),
            "Set index {:?} out of bounds for canvas of size {:?}",
            i,
            (self.width, self.height)
        );
        let (x, y) = self.i_to_xy(i);
        self.set(x, y, val);
    }

    pub fn is_in(&self, x: usize, y: usize) -> bool {
        x < self.width && y < self.height
    }

    pub fn is_in_i(&self, i: usize) -> bool {
        i < self.width * self.height
    }

    pub fn i_to_xy(&self, i: usize) -> (usize, usize) {
        let row = i / self.width;
        let col = i % self.height;
        (col, row)
    }

    /// Get a string representation of the canvas contents
    ///
    /// To deserialize, `insert` a serialized representation into a canvas of
    /// the original size.
    pub fn serialize(&self) -> String {
        let mut buf = String::with_capacity(self.width() * self.height());
        for y in 0..self.height() {
            for x in 0..self.width() {
                buf.push(*self.get(x, y));
            }
        }
        return buf;
    }
}

impl fmt::Display for Canvas {
    /// Nicely print the canvas as a grid of characters
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, row) in self.rows.iter().enumerate() {
            for cell in row {
                write!(f, "{}", cell)?
            }
            if i < self.height - 1 {
                write!(f, "\n")?
            }
        }
        Ok(())
    }
}

impl Index<usize> for Canvas {
    type Output = char;
    fn index(&self, i: usize) -> &Self::Output {
        self.geti(i)
    }
}

impl IndexMut<usize> for Canvas {
    fn index_mut(&mut self, i: usize) -> &mut Self::Output {
        self.geti_mut(i)
    }
}

impl Index<(usize, usize)> for Canvas {
    type Output = char;
    fn index(&self, (x, y): (usize, usize)) -> &Self::Output {
        self.get(x, y)
    }
}

impl IndexMut<(usize, usize)> for Canvas {
    fn index_mut(&mut self, (x, y): (usize, usize)) -> &mut Self::Output {
        self.get_mut(x, y)
    }
}

// Insertion options
impl Canvas {
    /// Add to the canvas from an iterable of chars
    fn insert_from_iter<I>(
        &mut self,
        source: &mut I,
        (start_x, start_y): (usize, usize),
        transparency: Option<char>,
    ) -> usize
    where
        I: Iterator<Item = char>,
    {
        let (mut x, mut y) = (start_x, start_y);
        let mut i = 0;
        for c in source {
            // new line
            if c == '\n' {
                y += 1;
                x = start_x;
                continue;
            }
            // if at end of row, new line
            if x >= self.width {
                y += 1;
                x = start_x;
            }
            // if at end of cols, stop and return
            if y >= self.height {
                break;
            }
            // set char, check if transparent
            if let Some(t) = transparency {
                if c == t {
                    x += 1;
                    i += 1;
                    continue;
                }
            }
            self.set(x, y, c);
            x += 1;
            i += 1;
        }
        return i;
    }

    /// Load a string into the canvas, wrapping on newlines
    pub fn insert(&mut self, s: &str) -> usize {
        self.insert_from_iter(&mut s.chars(), (0, 0), None)
    }

    /// Load characters from a reader into the canvas
    pub fn insert_from_read<R>(&mut self, r: R) -> io::Result<usize>
    where
        R: Read,
    {
        let mut err: Option<io::Error> = None;
        let size = self.insert_from_iter(
            &mut r.bytes().scan(0, |_, r| match r {
                Ok(b) => Some(b as char),
                Err(e) => {
                    err = Some(e);
                    None
                }
            }),
            (0, 0),
            None,
        );
        return match err {
            Some(e) => Err(e),
            None => Ok(size),
        };
    }
}

impl From<&str> for Canvas {
    /// Create a canvas from a string
    ///
    /// This determines the canvas dimensions based on the string, adding a row
    /// for each newline (except for a trailing one).
    fn from(s: &str) -> Self {
        // get dimensions from string
        let (width, height) = s.lines().fold((0, 0), |(w, h), line| {
            (if line.len() > w { line.len() } else { w }, h + 1)
        });
        // make canvas with dimensions
        let mut canvas = Canvas::new(width, height);
        // insert string to canvas
        canvas.insert(s);
        return canvas;
    }
}

impl Canvas {
    /// Get the characters of the canvas as a string, with line endings after each row.
    fn as_str(&self) -> String {
        let mut s = String::with_capacity(self.width() + 1 * self.height());
        for y in 0..self.height() {
            for x in 0..self.width() {
                s.push(*self.get(x, y));
            }
            s.push('\n');
        }
        return s;
    }
}

#[cfg(test)]
mod test {
    use super::Canvas;

    #[test]
    fn basics() {
        let mut c = Canvas::new(3, 4);
        // set upper left corner
        c.set(0, 0, 'A');
        assert_eq!(&'A', c.get(0, 0));

        // set lower right corner
        c.set(2, 3, 'B');
        assert_eq!(&'B', c.get(2, 3));
    }

    #[test]
    fn insert() {
        let s = "ABCDEFGH";

        let mut small = Canvas::new(2, 2);
        assert_eq!(4, small.insert(s), "Input string should be truncated");
        assert_eq!(&'A', small.get(0, 0));
        assert_eq!(&'B', small.get(1, 0));
        assert_eq!(&'C', small.get(0, 1));
        assert_eq!(&'D', small.get(1, 1));

        let mut large = Canvas::new(3, 3);
        assert_eq!(8, large.insert(s));
        let coords = [('A', 0, 0), ('C', 2, 0), ('H', 1, 2), (' ', 2, 2)];
        for &(c, x, y) in coords.iter() {
            assert_eq!(&c, large.get(x, y), "wrong value at ({}, {})", x, y)
        }

        let mut just_right = Canvas::new(4, 2);
        assert_eq!(8, just_right.insert(s));
        let coords = [('A', 0, 0), ('C', 2, 0), ('H', 3, 1)];
        for &(c, x, y) in coords.iter() {
            assert_eq!(&c, just_right.get(x, y), "wrong value at ({}, {})", x, y)
        }
    }

    #[test]
    fn from_str() {
        let s = "foobarflyer";
        let c = Canvas::from(s);
        assert_eq!(11, c.width());
        assert_eq!(1, c.height());
        assert_eq!(&'f', c.get(0, 0));
        assert_eq!(&'b', c.get(3, 0));

        let s = "foo\nbarfly\n\ner\n";
        let c = Canvas::from(s);
        assert_eq!(6, c.width());
        assert_eq!(4, c.height());
        assert_eq!(&'f', c.get(0, 0));
        assert_eq!(&'a', c.get(1, 1));
        assert_eq!(&' ', c.get(3, 2)); // blank line is all spaces
        assert_eq!(&'r', c.get(1, 3));
    }

    #[test]
    fn as_str() {
        let mut c = Canvas::new(2, 4);
        c.insert("foobar");
        let s = c.as_str();
        assert_eq!("fo\nob\nar\n  \n", s);
    }
}
