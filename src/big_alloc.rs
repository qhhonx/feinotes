//! Blocks of 1 MiB and up come straight from the kernel and go back to it
//! when freed, as on Windows (whose heap does this itself). The system
//! allocators here keep freed large blocks dirty for reuse: a decoded photo,
//! or a decode's scratch rows, stayed in the footprint long after its image
//! was gone (docs/MEMORY.md).

use std::alloc::{GlobalAlloc, Layout, System};
use std::ffi::c_void;

pub struct Alloc;

const BIG: usize = 1 << 20;
const PROT_READ_WRITE: i32 = 0x1 | 0x2;
const MAP_PRIVATE: i32 = 0x2;
#[cfg(target_os = "macos")]
const MAP_ANON: i32 = 0x1000;
#[cfg(not(target_os = "macos"))]
const MAP_ANON: i32 = 0x20;

unsafe extern "C" {
    fn mmap(addr: *mut c_void, len: usize, prot: i32, flags: i32, fd: i32, off: i64) -> *mut c_void;
    fn munmap(addr: *mut c_void, len: usize) -> i32;
}

/// Mapped memory is page aligned, which covers any smaller alignment.
fn big(l: &Layout) -> bool { l.size() >= BIG && l.align() <= 4096 }

unsafe fn map(size: usize) -> *mut u8 {
    let p = unsafe { mmap(std::ptr::null_mut(), size, PROT_READ_WRITE, MAP_PRIVATE | MAP_ANON, -1, 0) };
    if p as isize == -1 { std::ptr::null_mut() } else { p as *mut u8 }
}

unsafe impl GlobalAlloc for Alloc {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if big(&l) { unsafe { map(l.size()) } } else { unsafe { System.alloc(l) } }
    }
    // Fresh mappings are zeroed already.
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        if big(&l) { unsafe { map(l.size()) } } else { unsafe { System.alloc_zeroed(l) } }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        if big(&l) { unsafe { munmap(p as *mut c_void, l.size()); } } else { unsafe { System.dealloc(p, l) } }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, size: usize) -> *mut u8 {
        let to = unsafe { Layout::from_size_align_unchecked(size, l.align()) };
        if !big(&l) && !big(&to) { return unsafe { System.realloc(p, l, size) } }
        let q = unsafe { self.alloc(to) };
        if !q.is_null() {
            unsafe { std::ptr::copy_nonoverlapping(p, q, l.size().min(size)); self.dealloc(p, l); }
        }
        q
    }
}
