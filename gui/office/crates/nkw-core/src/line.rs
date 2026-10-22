//! A one-line text editor (the reply line of CONVERSA, the COMANDOS query):
//! readline's `ctrl+a`/`e`/`w`/`u`, arrows and backspace. Pure; the cursor
//! is a byte offset that always sits on a `char` boundary.

/// The text and where the cursor is (bytes into `text`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub cursor: usize,
}

impl Line {
    /// `text` with the cursor at its end.
    pub fn new(text: &str) -> Line {
        Line {
            text: text.to_string(),
            cursor: text.len(),
        }
    }

    /// The cursor, pulled back onto a char boundary inside the text (the
    /// fields are public).
    fn at(&self) -> usize {
        let mut c = self.cursor.min(self.text.len());
        while !self.text.is_char_boundary(c) {
            c -= 1;
        }
        c
    }

    /// Insert `s` at the cursor; the cursor moves past it.
    pub fn insert(&mut self, s: &str) {
        let c = self.at();
        self.text.insert_str(c, s);
        self.cursor = c + s.len();
    }

    /// Delete the char before the cursor.
    pub fn backspace(&mut self) {
        let c = self.at();
        if let Some((i, _)) = self.text[..c].char_indices().next_back() {
            self.text.replace_range(i..c, "");
            self.cursor = i;
        } else {
            self.cursor = c;
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.text.len();
    }

    /// `ctrl+w`: delete the word before the cursor (and the blanks between
    /// it and the cursor).
    pub fn delete_word(&mut self) {
        let c = self.at();
        let before = &self.text[..c];
        let trimmed = before.trim_end_matches(char::is_whitespace);
        let start = trimmed
            .char_indices()
            .rev()
            .find(|(_, ch)| ch.is_whitespace())
            .map(|(i, ch)| i + ch.len_utf8())
            .unwrap_or(0);
        self.text.replace_range(start..c, "");
        self.cursor = start;
    }

    /// `ctrl+u`: empty the line.
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    /// One char left.
    pub fn left(&mut self) {
        let c = self.at();
        self.cursor = self.text[..c]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i);
    }

    /// One char right.
    pub fn right(&mut self) {
        let c = self.at();
        self.cursor = self.text[c..]
            .chars()
            .next()
            .map_or(c, |ch| c + ch.len_utf8());
    }

    /// How many chars come before the cursor (where to draw it).
    pub fn cursor_chars(&self) -> usize {
        self.text[..self.at()].chars().count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `text` with `|` marking the cursor.
    fn show(l: &Line) -> String {
        let mut s = l.text.clone();
        s.insert(l.cursor, '|');
        s
    }

    fn parse(marked: &str) -> Line {
        let cursor = marked.find('|').unwrap();
        Line {
            text: marked.replacen('|', "", 1),
            cursor,
        }
    }

    #[test]
    fn ops_table() {
        type Op = fn(&mut Line);
        let rows: &[(&str, Op, &str)] = &[
            ("|", |l| l.insert("a"), "a|"),
            ("ab|", |l| l.insert("ç"), "abç|"),
            ("a|b", |l| l.insert("ão"), "aão|b"),
            ("ação|", Line::backspace, "açã|"),
            ("aç|ão", Line::backspace, "a|ão"),
            ("|ação", Line::backspace, "|ação"),
            ("", Line::backspace, ""),
            ("ação|", Line::left, "açã|o"),
            ("|ação", Line::left, "|ação"),
            ("a|ção", Line::right, "aç|ão"),
            ("ação|", Line::right, "ação|"),
            ("pé de |gato", Line::home, "|pé de gato"),
            ("|pé de gato", Line::end, "pé de gato|"),
            ("sim, aplicar agora|", Line::delete_word, "sim, aplicar |"),
            ("sim, aplicar   |", Line::delete_word, "sim, |"),
            ("sim|, aplicar", Line::delete_word, "|, aplicar"),
            ("    |", Line::delete_word, "|"),
            ("|abc", Line::delete_word, "|abc"),
            ("olá mundo|", Line::clear, "|"),
            ("é|", |l| l.insert("\n"), "é\n|"),
        ];
        for (from, op, want) in rows {
            let from = if from.is_empty() { "|" } else { from };
            let want = if want.is_empty() { "|" } else { want };
            let mut l = parse(from);
            op(&mut l);
            assert_eq!(show(&l), *want, "from {from:?}");
            assert!(l.text.is_char_boundary(l.cursor));
        }
    }

    #[test]
    fn a_cursor_off_a_boundary_is_pulled_back() {
        let mut l = Line {
            text: "ção".into(),
            cursor: 1, // inside 'ç'
        };
        l.insert("x");
        assert_eq!(show(&l), "x|ção");
        let mut l = Line {
            text: "ab".into(),
            cursor: 99,
        };
        l.backspace();
        assert_eq!(show(&l), "a|");
    }

    #[test]
    fn new_and_cursor_chars() {
        let l = Line::new("ação");
        assert_eq!(l.cursor, "ação".len());
        assert_eq!(l.cursor_chars(), 4);
        assert_eq!(parse("aç|ão").cursor_chars(), 2);
        assert_eq!(Line::default().cursor_chars(), 0);
    }
}
