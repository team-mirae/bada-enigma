//! bada enigma, a bada-compatible v1 IME engine (docs/bada-compatible-v1.md): henkan mode types
//! through an Enigma machine. Headless: event in → output out, no I/O.
//!
//! - [`compat`]: the contract layer, from team-mirae/bada-compatible. Not edited here.
//! - [`machine`]: the Enigma machine.
//! - [`Enigma`]: the IME.
//! - [`export!`] below: the C ABI, `include/enigma.h`, plus `enigma_machine` (below) for lampboards.
//! - [`wasm`]: on wasm32, what a web page needs to call that C ABI (memory for its strings).

pub mod compat;
mod enigma;
pub mod machine;
#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub use enigma::{Enigma, Form, Settings, Space};

export! {
    prefix: "enigma",
    ime: crate::Enigma,
    data: [],
    optional: [set_settings, host_selection, catalog, state],
}

/// Not part of bada-compatible: the machine as the engine's session left it (rotor windows, the
/// lit lamp, the setup), as JSON. The reply lives until the next call on any engine.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn enigma_machine(engine: *mut ffi::Engine) -> *const std::ffi::c_char {
    use std::cell::RefCell;
    use std::ffi::CString;
    thread_local! { static REPLY: RefCell<CString> = RefCell::new(CString::default()); }
    // SAFETY: a live handle from `enigma_new`, used by one thread at a time (spec §4).
    let Some(h) = (unsafe { engine.as_ref() }) else {
        return std::ptr::null();
    };
    let text = CString::new(h.ime.machine().to_string()).unwrap_or_default();
    REPLY.with(|r| {
        *r.borrow_mut() = text;
        r.borrow().as_ptr()
    })
}
