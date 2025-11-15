#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

use alloc::alloc::{alloc, dealloc};
use autheo_pqc_core::error::PqcError;
use autheo_pqc_core::handshake;
use core::alloc::Layout;
use core::slice;

#[cfg(target_arch = "wasm32")]
extern crate wee_alloc;

#[cfg(target_arch = "wasm32")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

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
/// Returns the number of bytes written to `resp_ptr` (>= 0) or a negative error code:
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
