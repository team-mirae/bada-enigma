//! The conformance checklist (docs/bada-compatible-v1.md §10), through the C ABI that `export!`
//! generates. A new IME keeps this file and changes the two lines below.

use std::ffi::{CStr, CString, c_char};
use std::thread;

use enigma::compat::{Output, STATE_FORMAT, State};
use serde_json::Value;

// ── For a new IME: its `ffi` module, and keys that stage something in henkan mode. ──
use enigma::ffi as abi;
const STAGE: &[&str] = &["KeyA", "KeyB"];

fn s(text: &str) -> CString {
    CString::new(text).unwrap()
}

fn json(ptr: *const c_char) -> Value {
    assert!(!ptr.is_null());
    let text = unsafe { CStr::from_ptr(ptr) }.to_str().expect("UTF-8");
    serde_json::from_str(text).unwrap_or_else(|e| panic!("not JSON ({e}): {text}"))
}

fn output(ptr: *const c_char) -> Output {
    serde_json::from_value(json(ptr)).expect("an Output")
}

fn key(e: *mut abi::Engine, code: &str) -> Output {
    output(abi::key(e, s(&format!(r#"{{"key":"{code}"}}"#)).as_ptr()))
}

struct Engine(*mut abi::Engine);

impl Engine {
    fn new() -> Engine {
        let e = abi::new(std::ptr::null());
        assert!(!e.is_null(), "P_new(NULL)");
        output(abi::set_mode(e, 1));
        Engine(e)
    }

    fn stage(&self) -> Output {
        let mut out = Output::default();
        for code in STAGE {
            out = key(self.0, code);
        }
        assert_ne!(out.preedit, "", "STAGE should leave a preedit");
        out
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        abi::free(self.0);
    }
}

#[test]
fn unknown_keys_and_bad_json_pass_on() {
    let e = Engine::new();
    for code in ["NoSuchKey", "", "Other", "F13", "MediaPlayPause", "CapsLock"] {
        assert!(!key(e.0, code).consumed, "{code:?}");
    }
    for bad in ["", "{", "null", r#"{"shift":true}"#, r#"{"key":7}"#] {
        let out = output(abi::key(e.0, s(bad).as_ptr()));
        assert!(!out.consumed, "{bad:?}");
    }
    assert!(!output(abi::key(e.0, std::ptr::null())).consumed);
}

#[test]
fn set_mode_commits_what_was_staged() {
    for mode in [0, 1, 2, u32::MAX] {
        let e = Engine::new();
        let staged = e.stage().preedit;
        let out = output(abi::set_mode(e.0, mode));
        if mode != 0 {
            // Henkan again: allowed to keep or commit, but never to lose it.
            assert!(out.commit == staged || out.preedit == staged, "mode {mode}: {out:?}");
        } else {
            assert_eq!((out.commit.as_str(), out.preedit.as_str()), (staged.as_str(), ""), "mode {mode}");
        }
    }
}

#[test]
fn flush_leaves_nothing_staged() {
    let e = Engine::new();
    let staged = e.stage().preedit;
    let out = output(abi::flush(e.0));
    assert_eq!((out.commit.as_str(), out.preedit.as_str()), (staged.as_str(), ""));
    let out = output(abi::flush(e.0));
    assert_eq!((out.commit.as_str(), out.preedit.as_str()), ("", ""));
}

#[test]
fn an_unused_key_commits_and_passes_on() {
    let e = Engine::new();
    let staged = e.stage().preedit;
    let out = key(e.0, "NoSuchKey");
    assert_eq!((out.commit.as_str(), out.preedit.as_str(), out.consumed), (staged.as_str(), "", false));
}

#[test]
fn replies_live_until_the_next_call() {
    let e = Engine::new();
    let first = abi::key(e.0, s(r#"{"key":"KeyA"}"#).as_ptr());
    let copy = unsafe { CStr::from_ptr(first) }.to_owned();
    // Still valid, and unchanged, before the next call.
    assert_eq!(unsafe { CStr::from_ptr(first) }, copy.as_c_str());
    // The argument was copied: freeing it changes nothing.
    let arg = s(r#"{"key":"KeyB"}"#);
    let second = abi::key(e.0, arg.as_ptr());
    drop(arg);
    json(second);
}

#[test]
fn engines_run_on_threads_at_once() {
    let handles: Vec<_> = (0..8)
        .map(|_| {
            thread::spawn(|| {
                let e = Engine::new();
                for _ in 0..200 {
                    e.stage();
                    output(abi::flush(e.0));
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn null_engine_is_null() {
    assert!(abi::key(std::ptr::null_mut(), s(r#"{"key":"KeyA"}"#).as_ptr()).is_null());
    assert!(abi::flush(std::ptr::null_mut()).is_null());
    abi::free(std::ptr::null_mut());
}

fn state(e: *mut abi::Engine) -> State {
    serde_json::from_value(json(abi::state(e))).expect("a State")
}

// Remove this test if your IME doesn't export `state`.
#[test]
fn state_reports_what_flush_would_commit() {
    let e = Engine::new();
    let idle = state(e.0);
    assert_eq!((idle.format.as_str(), idle.mode.as_deref(), idle.staged.as_str()), (STATE_FORMAT, Some("henkan"), ""));
    let out = e.stage();
    let s = state(e.0);
    assert_ne!(s.staged, "", "henkan with a preedit has something staged");
    assert_eq!(
        (&s.preedit, &s.menu, s.selected),
        (&out.preedit, &out.menu, out.selected),
        "display as the last Output"
    );
    assert_eq!(state(e.0), s, "asking changes nothing");
    key(e.0, "Tab");
    let s = state(e.0);
    assert_eq!(output(abi::flush(e.0)).commit, s.staged, "staged is what P_flush commits");
    assert_eq!(state(e.0).staged, "");
    output(abi::set_mode(e.0, 0));
    assert_eq!(state(e.0).mode.as_deref(), Some("muhenkan"));
    let fresh = abi::new(std::ptr::null());
    assert_eq!(state(fresh).mode, None, "no mode before the first P_set_mode");
    abi::free(fresh);
    assert!(abi::state(std::ptr::null_mut()).is_null());
}

#[test]
fn optional_functions_answer_json() {
    let e = Engine::new();
    #[allow(unused_variables)]
    let e0 = e.0;
    // Remove the lines for optional functions your IME doesn't export.
    let defaults = json(abi::set_settings(e0, s("{}").as_ptr()));
    assert_eq!(json(abi::set_settings(e0, s("not json").as_ptr())), defaults, "invalid JSON resets to defaults");
    assert_eq!(json(abi::set_settings(e0, s(r#"{"unknown": 1}"#).as_ptr())), defaults, "unknown fields are ignored");
    assert!(json(abi::catalog(e0)).is_object());
    output(abi::host_selection(e0, s("").as_ptr()));
    output(abi::host_selection(e0, std::ptr::null()));
}
