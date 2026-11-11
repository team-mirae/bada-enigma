//! The bada-compatible v1 layer (docs/bada-compatible-v1.md). **Don't edit this folder for your
//! IME**: implement [`Ime`] in your own module and export it with [`crate::export!`]. Changes here are
//! changes to the contract, and are made upstream (team-mirae/bada-compatible) for every IME.
//!
//! bada-compatible v1 is the C ABI of bada ime 0.2.0's engine (`engine/include/bada.h`): JSON key
//! events in, JSON [`Output`]s out, no I/O.

pub mod ffi;
pub mod host;
mod key;

use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use key::{Key, KeyEvent, Mods};

/// The contract version this layer implements (v1.1: v1 with the optional `P_state`).
pub const VERSION: u32 = 1;
pub const MINOR: u32 = 1;

/// The `format` of every [`State`] (spec §12).
pub const STATE_FORMAT: &str = "bada-compatible-state/1";

/// The two modes (spec §2). `set_mode(0)` is muhenkan; any other value is henkan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    /// No composition: keys type directly, or pass to the app.
    Muhenkan,
    /// Keys compose into a preedit, which is committed.
    Henkan,
}

impl Mode {
    pub fn from_u32(mode: u32) -> Mode {
        if mode == 0 { Mode::Muhenkan } else { Mode::Henkan }
    }

    pub fn as_u32(self) -> u32 {
        match self {
            Mode::Muhenkan => 0,
            Mode::Henkan => 1,
        }
    }
}

/// What the engine asks the host to do with one event (spec §6). bada ime's `engine::Output`, field
/// for field; every field is serialized, and a missing field reads as its default.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Output {
    /// Text to insert now: it replaces the marked text (the old preedit), or the selection if
    /// there is none or `replace_selection` is set.
    pub commit: String,
    pub replace_selection: bool,
    /// The marked text after the commit; `""`: nothing is marked.
    pub preedit: String,
    /// The menu's rows, top to bottom (negative entries first); empty: no menu.
    pub menu: Vec<String>,
    /// The highlighted row (an index into `menu`).
    pub selected: usize,
    /// The row of entry 0. Rows above it are negative entries (script forms).
    pub zero: usize,
    /// The engine took the key. false: the app gets it too, after the commit.
    pub consumed: bool,
    /// The menu is one row of single characters; `selected` past its end: none.
    pub horizontal: bool,
    /// Per menu row: a note shown beside it. Empty when no row has one.
    pub notes: Vec<String>,
    /// Per menu row: draw in a Japanese font. Empty when no row needs it.
    pub japanese: Vec<bool>,
    /// The engine needs the host's selected text to finish; the host answers with
    /// `host_selection`. The value names what the engine will do with it.
    pub need_selection: Option<String>,
}

impl Output {
    /// The key was taken, and nothing changes on screen beyond what the other fields say.
    pub fn consumed() -> Output {
        Output { consumed: true, ..Output::default() }
    }

    /// The key goes to the app, after `commit` is inserted.
    pub fn pass(commit: String) -> Output {
        Output { commit, ..Output::default() }
    }
}

/// What the engine reports about itself, for its wrapper (spec §12): `P_state`. Read-only: asking
/// changes nothing. The layer fills `mode` and the display fields (from the last Output); the IME
/// says what's staged ([`Report`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// [`STATE_FORMAT`].
    pub format: String,
    /// `"muhenkan"` or `"henkan"`; null before the first `P_set_mode`.
    pub mode: Option<String>,
    /// The henkan profile in use; null if the engine has none.
    pub profile: Option<String>,
    /// The text `P_flush` would commit now, as it would be typed (not the keys or tokens behind
    /// it). `""`: nothing is staged.
    pub staged: String,
    /// As the last Output left them.
    pub preedit: String,
    pub menu: Vec<String>,
    pub selected: usize,
    pub zero: usize,
    pub horizontal: bool,
    /// The engine is waiting for the host's selection.
    pub need_selection: Option<String>,
}

/// An [`Ime`] that reports its state: needed for `state` in `export!`'s `optional`.
pub trait Report: Ime {
    /// What [`Ime::flush`] would commit now, without committing: the text as it would be typed.
    /// `""` when nothing is staged. Must not change anything.
    fn staged(&self) -> String;

    /// The henkan profile in use, if the IME has profiles.
    fn profile(&self) -> Option<String> {
        None
    }
}

/// Settings for an IME without any: serializes as `{}` and accepts any object.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoSettings {}

/// An IME engine. Implement this, then export it with [`crate::export!`].
///
/// The required methods are the required functions of the contract (`P_new`, `P_key`,
/// `P_set_mode`, `P_flush`). The provided methods back the optional ones, and are only exported
/// when listed in `export!`'s `optional: [...]`: a host checks for each with `dlsym`, so an IME
/// that doesn't list one simply doesn't have it.
pub trait Ime: Send + 'static {
    /// Shared, immutable data (dictionaries, models), built once by `P_data_new` from the buffers
    /// the host loads, and shared by every engine, on any thread. `()` if the IME has none.
    /// `P_new(NULL)` (no `P_data_new`, or a host without data) gets `Data::default()`.
    type Data: Default + Send + Sync + 'static;

    /// The IME's settings. Unknown or missing fields take defaults (`#[serde(default)]`); invalid
    /// JSON resets to `Settings::default()`. [`NoSettings`] if the IME has none.
    type Settings: Default + Serialize + DeserializeOwned;

    /// A new engine. The host calls `set_mode` before the first key, so the starting mode is free.
    fn new(data: Arc<Self::Data>) -> Self;

    /// One key event. Unknown codes are [`Key::Other`]: commit what's staged if that's the rule,
    /// and pass the key on (`consumed: false`). Never panic.
    fn key(&mut self, event: &KeyEvent) -> Output;

    /// Commits what is staged (in `commit`), then switches mode.
    fn set_mode(&mut self, mode: Mode) -> Output;

    /// Commits what is staged, as when the text field loses focus. Leaves nothing staged.
    fn flush(&mut self) -> Output;

    /// `P_data_new`: builds the data from the buffers, in the order `export!` lists them. An
    /// empty buffer is `&[]`. Return `None` to fail (the host gets NULL). Must not keep the slices.
    fn data(buffers: &[&[u8]]) -> Option<Self::Data> {
        let _ = buffers;
        Some(Self::Data::default())
    }

    /// `P_set_settings`: replaces **all** settings. The host then gets [`Ime::settings`].
    fn set_settings(&mut self, settings: Self::Settings) {
        let _ = settings;
    }

    /// The settings in effect (after any clamping `set_settings` did).
    fn settings(&self) -> Self::Settings {
        Self::Settings::default()
    }

    /// `P_set_profile`: commits what's staged and switches henkan profile. Unknown names fall back
    /// to the default profile.
    fn set_profile(&mut self, name: &str) -> Output {
        let _ = name;
        self.flush()
    }

    /// `P_host_selection`: the host's answer to an Output's `need_selection` (`""`: nothing is
    /// selected, or something is marked).
    fn host_selection(&mut self, text: &str) -> Output {
        let _ = text;
        Output::consumed()
    }

    /// `P_catalog`: what the IME offers, for settings UIs and test planners. Free-form.
    fn catalog(&self) -> Value {
        Value::Object(Default::default())
    }

    /// `P_dictionary_load`: loads a user dictionary for the whole process (`""` clears it). Keep it
    /// in a `static` the engines read. Returns a summary, bada's shape:
    /// `{"ok", "error", "positive", "negative"}`.
    fn dictionary_load(json: &str) -> Value {
        let _ = json;
        serde_json::json!({"ok": false, "error": "this IME has no user dictionary"})
    }
}
