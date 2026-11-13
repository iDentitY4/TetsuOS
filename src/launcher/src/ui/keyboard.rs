//! An on-screen keyboard for typing a password with the D-pad. Rows of characters, then a
//! row of wide keys. A types the selected key, B deletes, X switches case, Y types a
//! space, Select switches to symbols and Start finishes.

use crate::i18n::tr;
use crate::ui::input::Button;

/// Characters per row, and so columns in the grid.
pub const COLUMNS: usize = 10;
/// The wide keys under the characters, each two columns wide.
pub const WIDE_KEYS: [Key; 5] = [Key::Case, Key::Symbols, Key::Space, Key::Delete, Key::Done];
pub const WIDE_SPAN: usize = COLUMNS / WIDE_KEYS.len();

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Layer {
    Lower,
    Upper,
    Symbols,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Key {
    Char(char),
    Case,
    Symbols,
    Space,
    Delete,
    Done,
}

impl Layer {
    /// Between them, the layers have every printable ASCII character but space, which
    /// has its own key.
    pub fn rows(self) -> [&'static str; 4] {
        match self {
            Self::Lower => ["1234567890", "qwertyuiop", "asdfghjkl-", "zxcvbnm_.@"],
            Self::Upper => ["1234567890", "QWERTYUIOP", "ASDFGHJKL-", "ZXCVBNM_.@"],
            Self::Symbols => ["!@#$%^&*()", "-_=+[]{}\\|", ";:'\",.<>/?", "`~"],
        }
    }
}

impl Key {
    /// The key's label, which depends on the layer it would switch from.
    pub fn label(self, layer: Layer) -> String {
        match self {
            Self::Char(c) => c.to_string(),
            Self::Case if layer == Layer::Upper => "abc".into(),
            Self::Case => "ABC".into(),
            Self::Symbols if layer == Layer::Symbols => "abc".into(),
            Self::Symbols => "#+=".into(),
            Self::Space => tr("SPACE").into(),
            Self::Delete => tr("DEL").into(),
            Self::Done => "OK".into(),
        }
    }
}

/// What the keyboard wants after a press.
#[derive(Clone, PartialEq, Debug)]
pub enum Outcome {
    Typing,
    Done(String),
    Cancel,
}

pub struct Keyboard {
    pub text: String,
    pub layer: Layer,
    /// 0 to 3 are character rows, 4 the wide keys.
    pub row: usize,
    /// A column of the character grid, even on the wide row, so moving up returns to it.
    pub column: usize,
    max_length: usize,
    /// Whether `Done` is allowed for the text so far.
    accepts: fn(&str) -> bool,
}

const WIDE_ROW: usize = 4;

impl Keyboard {
    pub fn new(max_length: usize, accepts: fn(&str) -> bool) -> Self {
        Self { text: String::new(), layer: Layer::Lower, row: 1, column: 0, max_length, accepts }
    }

    pub fn can_finish(&self) -> bool {
        (self.accepts)(&self.text)
    }

    fn row_length(&self, row: usize) -> usize {
        if row == WIDE_ROW {
            WIDE_KEYS.len()
        } else {
            self.layer.rows()[row].chars().count()
        }
    }

    /// The selected key's index within its row.
    pub fn selected_index(&self) -> usize {
        if self.row == WIDE_ROW {
            self.column / WIDE_SPAN
        } else {
            self.column.min(self.row_length(self.row) - 1)
        }
    }

    pub fn selected(&self) -> Key {
        self.key_at(self.row, self.selected_index())
    }

    pub fn key_at(&self, row: usize, index: usize) -> Key {
        if row == WIDE_ROW {
            WIDE_KEYS[index]
        } else {
            Key::Char(self.layer.rows()[row].chars().nth(index).unwrap_or(' '))
        }
    }

    fn type_key(&mut self, key: Key) -> Outcome {
        match key {
            Key::Char(c) => self.push(c),
            Key::Space => self.push(' '),
            // From symbols, the case key goes to capitals, as its label says.
            Key::Case => {
                self.layer = if self.layer == Layer::Upper { Layer::Lower } else { Layer::Upper };
            }
            Key::Symbols => {
                self.layer = if self.layer == Layer::Symbols { Layer::Lower } else { Layer::Symbols };
            }
            Key::Delete => {
                self.text.pop();
            }
            Key::Done if self.can_finish() => return Outcome::Done(self.text.clone()),
            Key::Done => {}
        }
        Outcome::Typing
    }

    fn push(&mut self, c: char) {
        if self.text.chars().count() < self.max_length {
            self.text.push(c);
        }
    }

    pub fn press(&mut self, button: Button) -> Outcome {
        match button {
            Button::Up => self.row = (self.row + WIDE_ROW) % (WIDE_ROW + 1),
            Button::Down => self.row = (self.row + 1) % (WIDE_ROW + 1),
            Button::Left | Button::Right => {
                let length = self.row_length(self.row);
                let index = self.selected_index();
                let index = if button == Button::Left { (index + length - 1) % length } else { (index + 1) % length };
                self.column = if self.row == WIDE_ROW { index * WIDE_SPAN } else { index };
            }
            Button::A => return self.type_key(self.selected()),
            Button::B if self.text.is_empty() => return Outcome::Cancel,
            Button::B => return self.type_key(Key::Delete),
            Button::X => return self.type_key(Key::Case),
            Button::Y => return self.type_key(Key::Space),
            Button::Select => return self.type_key(Key::Symbols),
            Button::Start => return self.type_key(Key::Done),
            _ => {}
        }
        Outcome::Typing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keyboard() -> Keyboard {
        Keyboard::new(63, |text| text.len() >= 3)
    }

    #[test]
    fn the_layers_cover_printable_ascii() {
        let all: String = [Layer::Lower, Layer::Upper, Layer::Symbols].iter().flat_map(|l| l.rows()).collect();
        for c in '!'..='~' {
            assert!(all.contains(c), "{c:?} is missing");
        }
        for layer in [Layer::Lower, Layer::Upper, Layer::Symbols] {
            assert!(layer.rows().iter().all(|row| row.chars().count() <= COLUMNS));
        }
    }

    #[test]
    fn typing_moving_and_switching_layers() {
        let mut keyboard = keyboard();
        assert_eq!(keyboard.selected(), Key::Char('q'));
        keyboard.press(Button::A);
        keyboard.press(Button::X);
        keyboard.press(Button::Right);
        keyboard.press(Button::A);
        keyboard.press(Button::Y);
        keyboard.press(Button::Select);
        keyboard.press(Button::Up);
        keyboard.press(Button::A);
        assert_eq!(keyboard.text, "qW @");
    }

    #[test]
    fn rows_wrap_and_short_rows_keep_the_column() {
        let mut keyboard = keyboard();
        keyboard.press(Button::Left);
        assert_eq!(keyboard.selected(), Key::Char('p'));
        keyboard.press(Button::Select);
        keyboard.press(Button::Down);
        keyboard.press(Button::Down);
        assert_eq!(keyboard.selected(), Key::Char('~'), "the short row's last key");
        keyboard.press(Button::Up);
        assert_eq!(keyboard.selected(), Key::Char('?'), "back in the column it left");
    }

    #[test]
    fn the_wide_row_maps_to_columns() {
        let mut keyboard = keyboard();
        for _ in 0..7 {
            keyboard.press(Button::Right);
        }
        keyboard.press(Button::Down);
        keyboard.press(Button::Down);
        keyboard.press(Button::Down);
        assert_eq!(keyboard.selected(), Key::Delete, "column 7 is under DEL");
        keyboard.press(Button::Right);
        assert_eq!(keyboard.selected(), Key::Done);
        keyboard.press(Button::Right);
        assert_eq!(keyboard.selected(), Key::Case, "wraps");
        keyboard.press(Button::Down);
        assert_eq!(keyboard.selected(), Key::Char('1'), "wraps to the top");
    }

    #[test]
    fn done_needs_acceptable_text_and_b_deletes_then_cancels() {
        let mut keyboard = keyboard();
        keyboard.press(Button::A);
        assert_eq!(keyboard.press(Button::Start), Outcome::Typing, "too short");
        keyboard.press(Button::A);
        keyboard.press(Button::A);
        assert_eq!(keyboard.press(Button::Start), Outcome::Done("qqq".into()));
        for _ in 0..3 {
            assert_eq!(keyboard.press(Button::B), Outcome::Typing);
        }
        assert_eq!(keyboard.press(Button::B), Outcome::Cancel);
    }

    #[test]
    fn the_two_layer_keys_never_share_a_label() {
        let mut keyboard = keyboard();
        for layer in [Layer::Lower, Layer::Upper, Layer::Symbols] {
            assert_ne!(Key::Case.label(layer), Key::Symbols.label(layer), "{layer:?}");
        }
        keyboard.press(Button::Select);
        keyboard.press(Button::X);
        assert_eq!(keyboard.layer, Layer::Upper, "the case key leaves symbols for capitals");
    }

    #[test]
    fn text_stops_at_the_maximum_length() {
        let mut keyboard = Keyboard::new(2, |_| true);
        for _ in 0..5 {
            keyboard.press(Button::A);
        }
        assert_eq!(keyboard.text, "qq");
    }
}
