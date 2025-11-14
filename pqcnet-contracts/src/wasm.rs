#![cfg(target_arch = "wasm32")]

use crate::error::PqcError;
use crate::handshake;
use alloc::alloc::{alloc, dealloc};
use core::alloc::Layout;
use core::slice;

/// Allocate linear memory for the host.
#[no_mangle]
pub extern "C" fn pqc_alloc(len: u32) -> u32 {
    if len == 0 {
        return 0;
    }

    let layout = match Layout::from_size_align(len as usize, 1) {
        Ok(layout) => layout,
        Err(_) => return 0,
    };

    let ptr = unsafe { alloc(layout) };
    if ptr.is_null() {
        0
    } else {
        ptr as u32
    }
}

/// Release previously allocated memory.
#[no_mangle]
pub extern "C" fn pqc_free(ptr: u32, len: u32) {
    if ptr == 0 || len == 0 {
        return;
    }

    if let Ok(layout) = Layout::from_size_align(len as usize, 1) {
        unsafe {
            dealloc(ptr as *mut u8, layout);
        }
    }
}

/// Entry-point used by the host runtime to perform a handshake.
///
/// # Parameters
/// * `req_ptr`, `req_len`   – request buffer allocated inside the WASM module.
/// * `resp_ptr`, `resp_len` – response buffer allocated inside the WASM module.
///
/// Returns the number of bytes written to `resp_ptr` (>= 0) or a negative error:
/// * `-1` – invalid input
/// * `-2` – response buffer too small
/// * `-127` – internal error
#[no_mangle]
pub extern "C" fn pqc_handshake(req_ptr: u32, req_len: u32, resp_ptr: u32, resp_len: u32) -> i32 {
    if req_ptr == 0 || resp_ptr == 0 {
        return -1;
    }

    let request = unsafe { slice::from_raw_parts(req_ptr as *const u8, req_len as usize) };
    let response = unsafe { slice::from_raw_parts_mut(resp_ptr as *mut u8, resp_len as usize) };

    match handshake::execute_handshake(request, response) {
        Ok(len) => len as i32,
        Err(PqcError::InvalidInput(_)) => -1,
        Err(PqcError::LimitExceeded(_)) => -2,
        Err(_) => -127,
    }
}
