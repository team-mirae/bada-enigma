//! The C ABI (spec §3, §4). [`crate::export!`] generates the `extern "C"` functions for an [`Ime`];
//! the generic code they call is here. Every function catches panics: nothing unwinds into the host.
//!
//! These take the host's raw pointers in safe functions, as `extern "C"` entry points do: the
//! contract (spec §4) is what makes them valid.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::{CStr, CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use super::{Ime, KeyEvent, Mode, Output, Report, STATE_FORMAT, State};

/// An engine as the host holds it: the IME and its last reply, which stays valid until the next
/// call on the same engine (spec §4).
pub struct Handle<I: Ime> {
    pub ime: I,
    reply: CString,
    /// The mode last set (`P_state`); None before the first `P_set_mode`.
    mode: Option<Mode>,
    /// The last Output, for `P_state`'s display fields.
    shown: Output,
}

fn guard<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(fallback)
}

/// A NUL-terminated argument, valid for the call. NULL or invalid UTF-8: `""`.
///
/// # Safety
/// `ptr` is NULL or a NUL-terminated string valid for the call.
pub unsafe fn cstr<'a>(ptr: *const c_char) -> &'a str {
    if ptr.is_null() {
        return "";
    }
    // SAFETY: by the caller.
    unsafe { CStr::from_ptr(ptr) }.to_str().unwrap_or("")
}

/// A data buffer, valid for the call. `(NULL, _)` or `(_, 0)`: empty.
///
/// # Safety
/// `ptr` is NULL or points to `len` bytes valid for the call.
pub unsafe fn slice<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if ptr.is_null() || len == 0 {
        return &[];
    }
    // SAFETY: by the caller.
    unsafe { std::slice::from_raw_parts(ptr, len) }
}

fn json(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".into())
}

/// Runs `f` on the engine and keeps its JSON as the reply. NULL engine: NULL. A panic: `fallback`.
fn reply<I: Ime>(h: *mut Handle<I>, fallback: &str, f: impl FnOnce(&mut I) -> String) -> *const c_char {
    // SAFETY: a live handle from `new`, used by one thread at a time (spec §4).
    let Some(h) = (unsafe { h.as_mut() }) else {
        return std::ptr::null();
    };
    let text = guard(fallback.to_string(), || f(&mut h.ime));
    h.reply = CString::new(text).unwrap_or_default();
    h.reply.as_ptr()
}

/// Like `reply`, for functions that return an Output; keeps it for `P_state`.
fn output<I: Ime>(h: *mut Handle<I>, f: impl FnOnce(&mut I) -> Output) -> *const c_char {
    // SAFETY: as in `reply`.
    let Some(h) = (unsafe { h.as_mut() }) else {
        return std::ptr::null();
    };
    let out = guard(None, || Some(f(&mut h.ime)));
    let text = out.as_ref().map(json).unwrap_or_else(|| "{}".into());
    if let Some(out) = out {
        h.shown = out;
    }
    h.reply = CString::new(text).unwrap_or_default();
    h.reply.as_ptr()
}

pub fn data_new<I: Ime>(buffers: &[&[u8]]) -> *const I::Data {
    guard(std::ptr::null(), || match I::data(buffers) {
        Some(d) => Arc::into_raw(Arc::new(d)),
        None => std::ptr::null(),
    })
}

pub fn data_free<I: Ime>(data: *const I::Data) {
    if !data.is_null() {
        // SAFETY: from `data_new`; every engine holds its own reference.
        guard((), || drop(unsafe { Arc::from_raw(data) }));
    }
}

pub fn new<I: Ime>(data: *const I::Data) -> *mut Handle<I> {
    guard(std::ptr::null_mut(), || {
        let data = if data.is_null() {
            Arc::new(I::Data::default())
        } else {
            // SAFETY: from `data_new`, alive until `data_free`; the engine keeps its own reference.
            unsafe {
                Arc::increment_strong_count(data);
                Arc::from_raw(data)
            }
        };
        Box::into_raw(Box::new(Handle {
            ime: I::new(data),
            reply: CString::default(),
            mode: None,
            shown: Output::default(),
        }))
    })
}

pub fn free<I: Ime>(h: *mut Handle<I>) {
    if !h.is_null() {
        // SAFETY: from `new`.
        guard((), || drop(unsafe { Box::from_raw(h) }));
    }
}

/// Invalid JSON: an Output with nothing in it (`consumed: false`), as bada does.
pub fn key<I: Ime>(h: *mut Handle<I>, event: *const c_char) -> *const c_char {
    // SAFETY: an argument string, valid for the call.
    let event = serde_json::from_str::<KeyEvent>(unsafe { cstr(event) });
    output(h, |ime| match event {
        Ok(e) => ime.key(&e),
        Err(_) => Output::default(),
    })
}

pub fn set_mode<I: Ime>(h: *mut Handle<I>, mode: u32) -> *const c_char {
    let mode = Mode::from_u32(mode);
    // SAFETY: as in `reply`.
    if let Some(h) = unsafe { h.as_mut() } {
        h.mode = Some(mode);
    }
    output(h, |ime| ime.set_mode(mode))
}

pub fn flush<I: Ime>(h: *mut Handle<I>) -> *const c_char {
    output(h, I::flush)
}

/// Invalid JSON resets to the defaults. Returns the settings in effect (not an Output).
pub fn set_settings<I: Ime>(h: *mut Handle<I>, settings: *const c_char) -> *const c_char {
    // SAFETY: an argument string, valid for the call.
    let settings = serde_json::from_str::<I::Settings>(unsafe { cstr(settings) }).unwrap_or_default();
    reply(h, "{}", |ime| {
        ime.set_settings(settings);
        json(&ime.settings())
    })
}

pub fn set_profile<I: Ime>(h: *mut Handle<I>, name: *const c_char) -> *const c_char {
    // SAFETY: an argument string, valid for the call.
    let name = unsafe { cstr(name) }.to_string();
    output(h, |ime| ime.set_profile(&name))
}

pub fn host_selection<I: Ime>(h: *mut Handle<I>, text: *const c_char) -> *const c_char {
    // SAFETY: an argument string, valid for the call.
    let text = unsafe { cstr(text) }.to_string();
    output(h, |ime| ime.host_selection(&text))
}

/// The engine's state (spec §12). Changes nothing but the reply buffer.
pub fn state<I: Report>(h: *mut Handle<I>) -> *const c_char {
    // SAFETY: as in `reply`.
    let Some(h) = (unsafe { h.as_mut() }) else {
        return std::ptr::null();
    };
    let (ime, shown) = (&h.ime, &h.shown);
    let text = guard("{}".to_string(), || {
        json(&State {
            format: STATE_FORMAT.into(),
            mode: h.mode.map(|m| if m == Mode::Muhenkan { "muhenkan" } else { "henkan" }.into()),
            profile: ime.profile(),
            staged: ime.staged(),
            preedit: shown.preedit.clone(),
            menu: shown.menu.clone(),
            selected: shown.selected,
            zero: shown.zero,
            horizontal: shown.horizontal,
            need_selection: shown.need_selection.clone(),
        })
    });
    h.reply = CString::new(text).unwrap_or_default();
    h.reply.as_ptr()
}

pub fn catalog<I: Ime>(h: *mut Handle<I>) -> *const c_char {
    reply(h, "{}", |ime| json(&ime.catalog()))
}

/// Process-wide. The caller frees the result with `P_string_free`.
pub fn dictionary_load<I: Ime>(text: *const c_char) -> *mut c_char {
    // SAFETY: an argument string, valid for the call.
    let text = unsafe { cstr(text) }.to_string();
    let summary = guard(r#"{"ok":false,"error":"panic"}"#.to_string(), || json(&I::dictionary_load(&text)));
    CString::new(summary).unwrap_or_default().into_raw()
}

pub fn string_free(text: *mut c_char) {
    if !text.is_null() {
        // SAFETY: from `dictionary_load`.
        drop(unsafe { CString::from_raw(text) });
    }
}

/// Exports an [`Ime`](crate::compat::Ime) as a bada-compatible v1 C ABI, in a module `ffi`.
///
/// ```ignore
/// crate::export! {                           // in src/lib.rs
///     prefix: "shout",                       // every symbol is shout_<name>
///     ime: crate::shout::Shout,
///     data: [(words, words_len), (lm, lm_len)], // P_data_new's buffers, in order; [] for none
///     optional: [set_settings, set_profile, host_selection, catalog, dictionary_load, state],
/// }
/// ```
///
/// The required functions (`P_new`, `P_free`, `P_key`, `P_set_mode`, `P_flush`) are always
/// exported. A non-empty `data` adds `P_data_new` and `P_data_free`; each name in `optional` adds
/// that function (`dictionary_load` adds `P_string_free` too; `state` needs the IME to implement
/// [`Report`](crate::compat::Report)). Leave out what the IME doesn't do:
/// hosts look for each optional function and do without it.
#[macro_export]
macro_rules! export {
    (
        prefix: $p:literal,
        ime: $ime:ty,
        data: [$(($buf:ident, $len:ident)),* $(,)?],
        optional: [$($opt:ident),* $(,)?] $(,)?
    ) => {
        /// The C ABI (`include/`), generated by `export!`. Rust callers (tests) may use it too.
        pub mod ffi {
            #![allow(clippy::not_unsafe_ptr_arg_deref)]
            #[allow(unused_imports)]
            use super::*;
            use ::std::ffi::c_char;
            use $crate::compat::ffi as rt;

            pub type This = $ime;
            pub type Data = <This as $crate::compat::Ime>::Data;
            pub type Engine = rt::Handle<This>;
            /// The symbol prefix.
            pub const PREFIX: &str = $p;

            /// A new engine on `data` (NULL: the IME's default data). NULL on failure.
            #[unsafe(export_name = concat!($p, "_new"))]
            pub extern "C" fn new(data: *const Data) -> *mut Engine { rt::new::<This>(data) }

            #[unsafe(export_name = concat!($p, "_free"))]
            pub extern "C" fn free(engine: *mut Engine) { rt::free(engine) }

            #[unsafe(export_name = concat!($p, "_key"))]
            pub extern "C" fn key(engine: *mut Engine, event: *const c_char) -> *const c_char {
                rt::key(engine, event)
            }

            #[unsafe(export_name = concat!($p, "_set_mode"))]
            pub extern "C" fn set_mode(engine: *mut Engine, mode: u32) -> *const c_char {
                rt::set_mode(engine, mode)
            }

            #[unsafe(export_name = concat!($p, "_flush"))]
            pub extern "C" fn flush(engine: *mut Engine) -> *const c_char { rt::flush(engine) }

            $crate::export!(@data $p, [$(($buf, $len)),*]);
            $($crate::export!(@opt $p, $opt);)*
        }
    };

    (@data $p:literal, []) => {};
    (@data $p:literal, [$(($buf:ident, $len:ident)),+]) => {
        /// Shared data from the buffers, in order. NULL on failure. Free with `data_free`.
        #[unsafe(export_name = concat!($p, "_data_new"))]
        pub extern "C" fn data_new($($buf: *const u8, $len: usize),+) -> *const Data {
            // SAFETY: the host passes (pointer, length) pairs valid for the call.
            rt::data_new::<This>(&[$(unsafe { rt::slice($buf, $len) }),+])
        }

        /// After every engine using it is freed.
        #[unsafe(export_name = concat!($p, "_data_free"))]
        pub extern "C" fn data_free(data: *const Data) { rt::data_free::<This>(data) }
    };

    (@opt $p:literal, set_settings) => {
        /// Settings JSON → the settings in effect.
        #[unsafe(export_name = concat!($p, "_set_settings"))]
        pub extern "C" fn set_settings(engine: *mut Engine, json: *const c_char) -> *const c_char {
            rt::set_settings(engine, json)
        }
    };
    (@opt $p:literal, set_profile) => {
        #[unsafe(export_name = concat!($p, "_set_profile"))]
        pub extern "C" fn set_profile(engine: *mut Engine, name: *const c_char) -> *const c_char {
            rt::set_profile(engine, name)
        }
    };
    (@opt $p:literal, host_selection) => {
        #[unsafe(export_name = concat!($p, "_host_selection"))]
        pub extern "C" fn host_selection(engine: *mut Engine, text: *const c_char) -> *const c_char {
            rt::host_selection(engine, text)
        }
    };
    (@opt $p:literal, state) => {
        /// The engine's state (spec §12), for the wrapper to expose. Read-only.
        #[unsafe(export_name = concat!($p, "_state"))]
        pub extern "C" fn state(engine: *mut Engine) -> *const c_char { rt::state(engine) }
    };
    (@opt $p:literal, catalog) => {
        #[unsafe(export_name = concat!($p, "_catalog"))]
        pub extern "C" fn catalog(engine: *mut Engine) -> *const c_char { rt::catalog(engine) }
    };
    (@opt $p:literal, dictionary_load) => {
        /// Process-wide. Free the summary with `string_free`.
        #[unsafe(export_name = concat!($p, "_dictionary_load"))]
        pub extern "C" fn dictionary_load(json: *const c_char) -> *mut c_char {
            rt::dictionary_load::<This>(json)
        }

        #[unsafe(export_name = concat!($p, "_string_free"))]
        pub extern "C" fn string_free(text: *mut c_char) { rt::string_free(text) }
    };
}
