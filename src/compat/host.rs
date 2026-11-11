//! A headless host for tests: a plain text field that applies [`Output`]s as spec §7 says, and
//! handles the language keys itself. It drives an [`Ime`] directly, without the C ABI; for the ABI,
//! see `tests/conformance.rs`, and for a full host over REST and MCP, superkeyboard.
//!
//! ```
//! use enigma::compat::host::Host;
//! let mut h = Host::<enigma::Enigma>::default();
//! h.type_keys("h i Space").unwrap();
//! assert_eq!(h.text(), "IO ");
//! ```

use std::sync::Arc;

use super::{Ime, Key, KeyEvent, Mode, Mods, Output};

/// The henkan profile かな selects (bada spec §6.6).
pub const MAC_KANA: &str = "mac-kana";

/// JIS legends (macOS ABC on a JIS keyboard): code, unshifted, shifted. Letters are separate.
const JIS: &[(&str, char, char)] = &[
    ("Digit1", '1', '!'),
    ("Digit2", '2', '"'),
    ("Digit3", '3', '#'),
    ("Digit4", '4', '$'),
    ("Digit5", '5', '%'),
    ("Digit6", '6', '&'),
    ("Digit7", '7', '\''),
    ("Digit8", '8', '('),
    ("Digit9", '9', ')'),
    ("Digit0", '0', '0'),
    ("Minus", '-', '='),
    ("Equal", '^', '~'),
    ("IntlYen", '¥', '|'),
    ("BracketLeft", '@', '`'),
    ("BracketRight", '[', '{'),
    ("Semicolon", ';', '+'),
    ("Quote", ':', '*'),
    ("Backslash", ']', '}'),
    ("Comma", ',', '<'),
    ("Period", '.', '>'),
    ("Slash", '/', '?'),
    ("IntlRo", '\\', '_'),
];

/// The character a plain JIS text field types for a key (None: not a character key).
pub fn jis_char(key: Key, shift: bool) -> Option<char> {
    if let Some(l) = key.letter() {
        return Some(if shift { l.to_ascii_uppercase() } else { l });
    }
    JIS.iter().find(|(c, _, _)| *c == key.code()).map(|&(_, b, s)| if shift { s } else { b })
}

/// The key (and ⇧) that types `ch` on JIS; unshifted preferred.
fn jis_key(ch: char) -> Option<(&'static str, bool)> {
    if ch.is_ascii_alphabetic() {
        let code = Key::ALL.iter().find(|k| k.letter() == Some(ch.to_ascii_lowercase()))?.code();
        return Some((code, ch.is_ascii_uppercase()));
    }
    JIS.iter()
        .find(|&&(_, b, _)| b == ch)
        .map(|&(c, _, _)| (c, false))
        .or_else(|| JIS.iter().find(|&&(_, _, s)| s == ch).map(|&(c, _, _)| (c, true)))
}

/// One token of the key notation: `[S-][C-][A-][M-]name`, where name is a `KeyboardEvent.code`
/// (`Space`, `KeyA`, `F7`), an alias (`kana` Lang1, `eisu` Lang2, `henkan` Convert, `katakana`
/// Lang3, `zenkaku` Backquote, `Esc`, `BS`) or one character typed on JIS (`a`, `A` is ⇧a, `?`).
pub fn parse_key(token: &str) -> Result<KeyEvent, String> {
    let mut mods = Mods::NONE;
    let mut rest = token;
    loop {
        match rest.get(..2) {
            Some("S-") if rest.len() > 2 => mods.shift = true,
            Some("C-") if rest.len() > 2 => mods.ctrl = true,
            Some("A-") if rest.len() > 2 => mods.alt = true,
            Some("M-") if rest.len() > 2 => mods.meta = true,
            _ => break,
        }
        rest = &rest[2..];
    }
    let code = match rest {
        "kana" | "かな" => "Lang1",
        "eisu" | "英数" => "Lang2",
        "henkan" | "変換" => "Convert",
        "katakana" | "カタカナ" => "Lang3",
        "zenkaku" | "半角/全角" => "Backquote",
        "Esc" => "Escape",
        "BS" => "Backspace",
        _ => {
            let mut chars = rest.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => {
                    let (code, shift) = jis_key(c).ok_or_else(|| format!("no JIS key types {c:?}"))?;
                    mods.shift |= shift;
                    code
                }
                _ if Key::from_code(rest) != Key::Other => rest,
                _ => return Err(format!("unknown key {token:?}")),
            }
        }
    };
    Ok(KeyEvent::new(code, mods))
}

/// A text field with an IME: characters, a selection `(start, end)` (equal: a caret) and an
/// optional marked range `(start, length)`, all in `char`s.
pub struct Host<I: Ime> {
    pub ime: I,
    text: Vec<char>,
    sel: (usize, usize),
    marked: Option<(usize, usize)>,
    menu: Vec<String>,
    selected: usize,
    zero: usize,
    mode: Mode,
    profile: String,
    /// Every Output the host applied, in order (cleared by nothing: tests read it).
    pub outputs: Vec<Output>,
}

impl<I: Ime> Default for Host<I> {
    /// Default data, in henkan mode.
    fn default() -> Self {
        Host::new(Arc::new(I::Data::default()), Mode::Henkan)
    }
}

impl<I: Ime> Host<I> {
    pub fn new(data: Arc<I::Data>, mode: Mode) -> Self {
        let mut h = Host {
            ime: I::new(data),
            text: Vec::new(),
            sel: (0, 0),
            marked: None,
            menu: Vec::new(),
            selected: 0,
            zero: 0,
            mode,
            profile: MAC_KANA.to_string(),
            outputs: Vec::new(),
        };
        h.switch_mode(mode);
        h
    }

    /// Everything in the field, the preedit included.
    pub fn text(&self) -> String {
        self.text.iter().collect()
    }

    /// The field without the marked text.
    pub fn committed(&self) -> String {
        match self.marked {
            Some((s, l)) => self.text[..s].iter().chain(&self.text[s + l..]).collect(),
            None => self.text(),
        }
    }

    pub fn preedit(&self) -> String {
        self.marked.map(|(s, l)| self.text[s..s + l].iter().collect()).unwrap_or_default()
    }

    pub fn menu(&self) -> &[String] {
        &self.menu
    }

    /// The highlighted entry, signed: 0 is the top-ranked reading, negatives are script forms.
    pub fn entry(&self) -> isize {
        self.selected as isize - self.zero as isize
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn profile(&self) -> &str {
        &self.profile
    }

    pub fn selection(&self) -> (usize, usize) {
        self.sel
    }

    pub fn last(&self) -> Option<&Output> {
        self.outputs.last()
    }

    /// Replaces all settings; returns the settings in effect.
    pub fn set_settings(&mut self, settings: I::Settings) -> I::Settings {
        self.ime.set_settings(settings);
        self.ime.settings()
    }

    /// A click or an app edit: commits what's staged first (spec §7.6).
    pub fn select(&mut self, start: usize, end: usize) {
        self.flush();
        let n = self.text.len();
        self.sel = (start.min(n), end.min(n).max(start.min(n)));
    }

    /// Replaces the field's text (the app did it), caret at the end. Commits what's staged first.
    pub fn set_text(&mut self, text: &str) {
        self.flush();
        self.text = text.chars().collect();
        self.sel = (self.text.len(), self.text.len());
    }

    /// Focus lost.
    pub fn flush(&mut self) {
        let out = self.ime.flush();
        self.apply(out, false);
    }

    /// Presses each key of the notation ([`parse_key`]), separated by whitespace.
    pub fn type_keys(&mut self, notation: &str) -> Result<(), String> {
        for token in notation.split_whitespace() {
            let event = parse_key(token)?;
            self.press(&event);
        }
        Ok(())
    }

    /// One key press (spec §7.2).
    pub fn press(&mut self, event: &KeyEvent) {
        if self.language_key(event) {
            return;
        }
        let out = self.ime.key(event);
        let consumed = out.consumed;
        self.apply(out, false);
        if !consumed {
            self.pass_on(event);
        }
    }

    /// The language keys (spec §7.4). True: handled here.
    fn language_key(&mut self, e: &KeyEvent) -> bool {
        let m = e.mods;
        let profile = match e.key() {
            Key::Lang2 => {
                self.switch_mode(Mode::Muhenkan);
                return true;
            }
            Key::Lang1 if m.ctrl && m.alt && m.shift => {
                let other = if self.mode == Mode::Henkan { Mode::Muhenkan } else { Mode::Henkan };
                self.switch_mode(other);
                return true;
            }
            Key::Lang1 => MAC_KANA,
            Key::Convert => "henkan",
            Key::Lang3 => "katakana",
            _ => return false,
        };
        if self.mode == Mode::Muhenkan {
            self.profile = profile.to_string();
            self.switch_mode(Mode::Henkan);
        } else if profile == self.profile || (e.key() == Key::Lang1 && self.menu.len() > 1) {
            let out = self.ime.key(&KeyEvent::new("Lang1", Mods { shift: m.shift, ..Mods::NONE }));
            self.apply(out, false);
        } else {
            self.profile = profile.to_string();
            let out = self.ime.set_profile(profile);
            self.apply(out, false);
        }
        true
    }

    fn switch_mode(&mut self, mode: Mode) {
        let out = self.ime.set_mode(mode);
        self.apply(out, false);
        self.mode = mode;
        let out = self.ime.set_profile(&self.profile.clone());
        self.apply(out, false);
    }

    fn replace(&mut self, start: usize, end: usize, with: &str) {
        let new: Vec<char> = with.chars().collect();
        let n = new.len();
        self.text.splice(start..end, new);
        self.sel = (start + n, start + n);
    }

    /// Applies an Output (spec §7.1). `answering`: this is the reply to a `need_selection`.
    pub fn apply(&mut self, out: Output, answering: bool) {
        if !out.commit.is_empty() {
            let (s, e) = match self.marked {
                Some((s, l)) if !out.replace_selection => (s, s + l),
                _ => self.sel,
            };
            self.replace(s, e, &out.commit);
            self.marked = None;
        }
        if !out.preedit.is_empty() {
            let (s, e) = self.marked.map_or(self.sel, |(s, l)| (s, s + l));
            self.replace(s, e, &out.preedit);
            self.marked = Some((s, out.preedit.chars().count()));
        } else if let Some((s, l)) = self.marked.take() {
            self.replace(s, s + l, "");
        }
        self.menu = out.menu.clone();
        self.selected = out.selected;
        self.zero = out.zero;
        let need = out.need_selection.is_some() && !answering;
        self.outputs.push(out);
        if need {
            let text: String = match self.marked {
                None => self.text[self.sel.0..self.sel.1].iter().collect(),
                Some(_) => String::new(),
            };
            let reply = self.ime.host_selection(&text);
            self.apply(reply, true);
        }
    }

    /// What a plain text field does with a key the IME passed on (spec §7.3).
    fn pass_on(&mut self, e: &KeyEvent) {
        if !e.mods.is_plain() {
            return;
        }
        let (s, end) = self.sel;
        let line_start = |t: &[char], at: usize| t[..at].iter().rposition(|&c| c == '\n').map_or(0, |i| i + 1);
        match e.key() {
            Key::Space => self.replace(s, end, " "),
            Key::Tab => self.replace(s, end, "\t"),
            Key::Enter => self.replace(s, end, "\n"),
            Key::Backspace if s != end => self.replace(s, end, ""),
            Key::Backspace if s > 0 => self.replace(s - 1, s, ""),
            Key::Delete if s != end => self.replace(s, end, ""),
            Key::Delete if s < self.text.len() => self.replace(s, s + 1, ""),
            Key::ArrowLeft => {
                let at = if s == end { s.saturating_sub(1) } else { s };
                self.sel = (at, at);
            }
            Key::ArrowRight => {
                let at = if s == end { (end + 1).min(self.text.len()) } else { end };
                self.sel = (at, at);
            }
            Key::ArrowUp => self.sel = (0, 0),
            Key::ArrowDown => self.sel = (self.text.len(), self.text.len()),
            Key::Home => {
                let at = line_start(&self.text, s);
                self.sel = (at, at);
            }
            Key::End => {
                let at = self.text[end..].iter().position(|&c| c == '\n').map_or(self.text.len(), |i| end + i);
                self.sel = (at, at);
            }
            k => {
                if let Some(c) = jis_char(k, e.mods.shift) {
                    self.replace(s, end, &c.to_string());
                }
            }
        }
    }
}
