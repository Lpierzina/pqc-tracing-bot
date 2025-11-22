#![no_std]

extern crate alloc;

use alloc::alloc::{alloc, dealloc};
use chacha20::cipher::generic_array::GenericArray;
use chacha20::cipher::{KeyIvInit, StreamCipher, StreamCipherSeek};
use chacha20::ChaCha20;
use core::alloc::Layout;
use core::slice;
use sha2::{Digest, Sha512};
use spin::Mutex;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

const STATUS_OK: i32 = 0;
const ERR_BAD_PTR: i32 = -1;
const ERR_UNSEEDED: i32 = -2;

static RNG: Mutex<NodeRng> = Mutex::new(NodeRng::new());

struct NodeRng {
    key: [u8; 32],
    nonce: [u8; 12],
    cursor: u64,
    seeded: bool,
}

impl NodeRng {
    const fn new() -> Self {
        Self {
            key: [0u8; 32],
            nonce: [0u8; 12],
            cursor: 0,
            seeded: false,
        }
    }

    fn reseed(&mut self, seed: &[u8]) {
        let mut hasher = Sha512::new();
        hasher.update(seed);
        let digest = hasher.finalize();
        self.key.copy_from_slice(&digest[..32]);
        self.nonce.copy_from_slice(&digest[32..44]);
        self.cursor = 0;
        self.seeded = true;
    }

    fn fill(&mut self, dest: &mut [u8]) -> i32 {
        if dest.is_empty() {
            return STATUS_OK;
        }
        if !self.seeded {
            return ERR_UNSEEDED;
        }
        let key = GenericArray::from_slice(&self.key);
        let nonce = GenericArray::from_slice(&self.nonce);
        let mut cipher = ChaCha20::new(key, nonce);
        cipher.seek(self.cursor);
        cipher.apply_keystream(dest);
        self.cursor = self.cursor.wrapping_add(dest.len() as u64);
        STATUS_OK
    }
}

#[no_mangle]
pub extern "C" fn autheo_entropy_alloc(len: u32) -> u32 {
    if len == 0 {
        return 0;
    }
    let layout = match Layout::from_size_align(len as usize, 1) {
        Ok(layout) => layout,
        Err(_) => return 0,
    };
    unsafe {
        let ptr = alloc(layout);
        if ptr.is_null() {
            0
        } else {
            ptr as u32
        }
    }
}

#[no_mangle]
pub extern "C" fn autheo_entropy_free(ptr: u32, len: u32) {
    if ptr == 0 || len == 0 {
        return;
    }
    if let Ok(layout) = Layout::from_size_align(len as usize, 1) {
        unsafe {
            dealloc(ptr as *mut u8, layout);
        }
    }
}

#[no_mangle]
pub extern "C" fn autheo_entropy_seed(ptr: u32, len: u32) -> i32 {
    if ptr == 0 || len == 0 {
        return ERR_BAD_PTR;
    }
    let seed = unsafe { slice::from_raw_parts(ptr as *const u8, len as usize) };
    RNG.lock().reseed(seed);
    STATUS_OK
}

#[no_mangle]
pub extern "C" fn autheo_entropy_fill(ptr: u32, len: u32) -> i32 {
    if ptr == 0 {
        return ERR_BAD_PTR;
    }
    let dest = unsafe { slice::from_raw_parts_mut(ptr as *mut u8, len as usize) };
    RNG.lock().fill(dest)
}

#[no_mangle]
pub extern "C" fn autheo_entropy_health() -> i32 {
    if RNG.lock().seeded {
        STATUS_OK
    } else {
        ERR_UNSEEDED
    }
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[cfg_attr(target_arch = "wasm32", panic_handler)]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
