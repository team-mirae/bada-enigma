//! The C ABI, as is, on WebAssembly. A page writes NUL-terminated UTF-8 into memory from
//! `wasm_alloc`, passes the pointer to `enigma_key` and the rest, reads the reply up to its NUL,
//! and frees its argument with `wasm_free`. Nothing here is specific to bada enigma: any
//! bada-compatible engine can add this file and run in a browser.

/// `len` bytes for the page to write into.
#[unsafe(no_mangle)]
pub extern "C" fn wasm_alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len.max(1));
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Frees a buffer from `wasm_alloc(len)`.
#[unsafe(no_mangle)]
pub extern "C" fn wasm_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        // SAFETY: from `wasm_alloc(len)`.
        drop(unsafe { Vec::from_raw_parts(ptr, 0, len.max(1)) });
    }
}
