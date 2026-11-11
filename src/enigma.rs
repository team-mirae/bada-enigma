//! bada enigma, the IME. Henkan mode runs every letter through an Enigma machine.
//!
//! - **A henkan session** is one preedit: it starts at the first letter and ends when it is
//!   committed. Every session starts the machine from the starting state in the settings (rotors,
//!   reflector, rings, start positions, plugboard), so the same word always comes out the same.
//! - **Backspace** removes the last letter and turns the rotors back with it.
//! - The menu offers the ciphertext's forms (CAPITALS, lowercase, groups of five; `plain` shows
//!   what you typed). Tab / ↓ / かな move down, ⇧Tab / ↑ up. Space, Enter, Esc and any other key
//!   commit what shows, then act; ⇧Enter commits only. With `"space": "x"`, Space types an X into
//!   the message instead, as the operators did.
//! - Muhenkan: nothing is consumed, except 半角/全角, which enciphers the selection (from the
//!   starting state; case and anything that isn't a letter kept).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::compat::{Ime, Key, KeyEvent, Mode, Output, Report};
use crate::machine::{Machine, REFLECTORS, ROTORS};

/// One way to write the ciphertext: one menu row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Form {
    Upper,
    Lower,
    Groups,
    Plain,
}

impl Form {
    pub const ALL: [Form; 4] = [Form::Upper, Form::Lower, Form::Groups, Form::Plain];

    fn label(self) -> &'static str {
        match self {
            Form::Upper => "CAPITALS",
            Form::Lower => "lowercase",
            Form::Groups => "Groups of five",
            Form::Plain => "Plaintext (cheating)",
        }
    }
}

/// What Space does in a session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Space {
    /// Commits, then types a space. The next word is a new session.
    #[default]
    Commit,
    /// Types an X into the message.
    X,
}

/// The settings. Invalid values fall back to the defaults, field by field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Three different rotors from I–VIII, left to right.
    pub rotors: Vec<String>,
    /// A, B or C.
    pub reflector: String,
    /// Ring settings (Ringstellung), left to right: three letters.
    pub rings: String,
    /// The starting state: the letters in the windows when a session starts.
    pub start: String,
    /// Plugboard pairs (Steckerverbindungen), such as "AV BS CG"; at most 13, no letter twice.
    pub plugboard: String,
    /// The menu's rows, top first. The first shows while typing.
    pub forms: Vec<Form>,
    pub space: Space,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            rotors: vec!["I".into(), "II".into(), "III".into()],
            reflector: "B".into(),
            rings: "AAA".into(),
            start: "AAA".into(),
            plugboard: String::new(),
            forms: vec![Form::Upper, Form::Lower, Form::Groups],
            space: Space::Commit,
        }
    }
}

fn letters3(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().to_ascii_uppercase();
    let b = s.as_bytes();
    (b.len() == 3 && b.iter().all(u8::is_ascii_uppercase)).then(|| [b[0] - b'A', b[1] - b'A', b[2] - b'A'])
}

/// The pairs that are valid, in order; letters already plugged are skipped.
fn pairs(s: &str) -> Vec<(u8, u8)> {
    let mut used = [false; 26];
    let mut out = Vec::new();
    for p in s.to_ascii_uppercase().split(|c: char| !c.is_ascii_alphabetic()) {
        let b = p.as_bytes();
        if b.len() != 2 || b[0] == b[1] || out.len() == 13 {
            continue;
        }
        let (x, y) = (b[0] - b'A', b[1] - b'A');
        if used[x as usize] || used[y as usize] {
            continue;
        }
        used[x as usize] = true;
        used[y as usize] = true;
        out.push((x, y));
    }
    out
}

impl Settings {
    /// Each field valid, or its default; the plugboard normalized.
    fn clean(mut self) -> Settings {
        let d = Settings::default();
        let rotors: Vec<_> = self
            .rotors
            .iter()
            .filter_map(|r| ROTORS.iter().position(|(name, _, _)| name.eq_ignore_ascii_case(r.trim())))
            .collect();
        let distinct = rotors.len() == 3 && rotors[0] != rotors[1] && rotors[0] != rotors[2] && rotors[1] != rotors[2];
        self.rotors = if distinct { rotors.iter().map(|&i| ROTORS[i].0.to_string()).collect() } else { d.rotors };
        self.reflector = REFLECTORS
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(self.reflector.trim()))
            .map_or(d.reflector, |(name, _)| name.to_string());
        self.rings = letters3(&self.rings).map_or(d.rings, |_| self.rings.trim().to_ascii_uppercase());
        self.start = letters3(&self.start).map_or(d.start, |_| self.start.trim().to_ascii_uppercase());
        self.plugboard = pairs(&self.plugboard)
            .iter()
            .map(|&(a, b)| format!("{}{}", (b'A' + a) as char, (b'A' + b) as char))
            .collect::<Vec<_>>()
            .join(" ");
        let mut seen = Vec::new();
        self.forms.retain(|f| {
            !seen.contains(f) && {
                seen.push(*f);
                true
            }
        });
        if self.forms.is_empty() {
            self.forms = d.forms;
        }
        self
    }

    /// A machine in the starting state. The settings must be clean.
    fn machine(&self) -> Machine {
        let rotor = |i: usize| ROTORS.iter().position(|(n, _, _)| *n == self.rotors[i]).unwrap_or(i);
        let reflector = REFLECTORS.iter().position(|(n, _)| *n == self.reflector).unwrap_or(1);
        Machine::new(
            [rotor(0), rotor(1), rotor(2)],
            reflector,
            letters3(&self.rings).unwrap_or_default(),
            letters3(&self.start).unwrap_or_default(),
            &pairs(&self.plugboard),
        )
    }
}

pub struct Enigma {
    mode: Mode,
    settings: Settings,
    /// The machine as the session left it.
    machine: Machine,
    /// The session's plaintext, lowercase.
    plain: String,
    /// The session's ciphertext, uppercase.
    cipher: String,
    /// The menu row showing.
    row: usize,
    /// Waiting for the host's selection (半角/全角 in muhenkan).
    waiting: bool,
}

impl Enigma {
    fn write(&self, form: Form) -> String {
        match form {
            Form::Upper => self.cipher.clone(),
            Form::Lower => self.cipher.to_ascii_lowercase(),
            Form::Plain => self.plain.clone(),
            Form::Groups => {
                let b = self.cipher.as_bytes();
                b.chunks(5).map(|c| String::from_utf8_lossy(c).into_owned()).collect::<Vec<_>>().join(" ")
            }
        }
    }

    /// An Output for the current state, after `commit`.
    fn out(&self, commit: String, consumed: bool) -> Output {
        if self.plain.is_empty() {
            return Output { commit, consumed, ..Output::default() };
        }
        Output {
            commit,
            preedit: self.write(self.settings.forms[self.row]),
            menu: self.settings.forms.iter().map(|&f| self.write(f)).collect(),
            notes: self.settings.forms.iter().map(|f| f.label().to_string()).collect(),
            selected: self.row,
            consumed,
            ..Output::default()
        }
    }

    /// Ends the session: what shows, and the machine back to the starting state.
    fn take(&mut self) -> String {
        let shown = self.staged();
        self.plain.clear();
        self.cipher.clear();
        self.row = 0;
        self.machine = self.settings.machine();
        shown
    }

    fn press(&mut self, letter: char) {
        self.plain.push(letter);
        self.cipher.push(self.machine.press_char(letter.to_ascii_uppercase()));
        self.row = 0;
    }

    /// Unwinds the last letter: the machine is run again from the starting state.
    fn unpress(&mut self) {
        self.plain.pop();
        self.cipher.pop();
        self.machine = self.settings.machine();
        for c in self.plain.chars() {
            self.machine.press_char(c);
        }
        self.row = 0;
    }

    /// The machine for a lampboard: the letters in the windows, the lamp lit (the last letter's
    /// ciphertext), and what's set up. Not part of bada-compatible: `enigma_machine` in the C ABI.
    pub fn machine(&self) -> Value {
        json!({
            "window": self.machine.window(),
            "lamp": self.cipher.chars().last().map(String::from),
            "key": self.plain.chars().last().map(|c| c.to_ascii_uppercase().to_string()),
            "count": self.plain.len(),
            "rotors": self.settings.rotors,
            "reflector": self.settings.reflector,
            "rings": self.settings.rings,
            "start": self.settings.start,
            "plugboard": self.settings.plugboard,
        })
    }
}

impl Report for Enigma {
    /// What shows: the form on the selected row.
    fn staged(&self) -> String {
        if self.plain.is_empty() { String::new() } else { self.write(self.settings.forms[self.row]) }
    }
}

impl Ime for Enigma {
    type Data = ();
    type Settings = Settings;

    fn new(_data: Arc<()>) -> Self {
        let settings = Settings::default();
        Enigma {
            mode: Mode::Henkan,
            machine: settings.machine(),
            settings,
            plain: String::new(),
            cipher: String::new(),
            row: 0,
            waiting: false,
        }
    }

    fn key(&mut self, e: &KeyEvent) -> Output {
        let key = e.key();
        let m = e.mods;
        if self.mode == Mode::Muhenkan {
            if key == Key::Backquote && m.is_plain() {
                self.waiting = true;
                return Output { consumed: true, need_selection: Some("enigma".into()), ..Output::default() };
            }
            return Output::default();
        }
        let staged = !self.plain.is_empty();
        let last = self.settings.forms.len() - 1;
        match key {
            k if k.letter().is_some() && m.is_plain() => {
                self.press(k.letter().unwrap_or('x'));
                self.out(String::new(), true)
            }
            Key::Space if staged && m.is_plain() && self.settings.space == Space::X => {
                self.press('x');
                self.out(String::new(), true)
            }
            Key::Backspace if staged && m.is_plain() => {
                self.unpress();
                self.out(String::new(), true)
            }
            Key::Tab | Key::Lang1 if staged && !m.shift => {
                self.row = (self.row + 1).min(last);
                self.out(String::new(), true)
            }
            Key::ArrowDown if staged => {
                self.row = (self.row + 1).min(last);
                self.out(String::new(), true)
            }
            Key::Tab | Key::Lang1 | Key::ArrowUp if staged => {
                self.row = self.row.saturating_sub(1);
                self.out(String::new(), true)
            }
            Key::Enter if staged && m.shift => {
                let commit = self.take();
                self.out(commit, true)
            }
            Key::Lang1 => Output::consumed(),
            _ => {
                let commit = self.take();
                self.out(commit, false)
            }
        }
    }

    fn set_mode(&mut self, mode: Mode) -> Output {
        let commit = self.take();
        self.mode = mode;
        self.waiting = false;
        self.out(commit, false)
    }

    fn flush(&mut self) -> Output {
        let commit = self.take();
        self.out(commit, false)
    }

    /// The session goes on under the new settings: its letters are enciphered again.
    fn set_settings(&mut self, settings: Settings) {
        self.settings = settings.clean();
        let plain = std::mem::take(&mut self.plain);
        let row = self.row;
        self.take();
        for c in plain.chars() {
            self.press(c);
        }
        self.row = row.min(self.settings.forms.len() - 1);
    }

    fn settings(&self) -> Settings {
        self.settings.clone()
    }

    fn host_selection(&mut self, text: &str) -> Output {
        let waiting = std::mem::take(&mut self.waiting);
        if !waiting || text.is_empty() {
            return Output::consumed();
        }
        let mut machine = self.settings.machine();
        let commit = text.chars().map(|c| machine.press_char(c)).collect();
        Output { commit, replace_selection: true, consumed: true, ..Output::default() }
    }

    fn catalog(&self) -> Value {
        let forms: Vec<_> = Form::ALL.iter().map(|f| json!({"id": f, "label": f.label()})).collect();
        let rotors: Vec<_> = ROTORS.iter().map(|(n, w, t)| json!({"name": n, "wiring": w, "turnover": t})).collect();
        let reflectors: Vec<_> = REFLECTORS.iter().map(|(n, w)| json!({"name": n, "wiring": w})).collect();
        json!({"name": "bada enigma", "forms": forms, "rotors": rotors, "reflectors": reflectors, "space": ["commit", "x"]})
    }
}
