use std::fs::File;
use std::io::{self};
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::sync::atomic::{AtomicUsize, Ordering, AtomicU32};
use nix::sys::memfd::{memfd_create, MemFdCreateFlag};
use std::ffi::CString;

const SHM_SIZE: usize = 4096;

// Futex constants
const FUTEX_WAIT: i32 = 0;
const FUTEX_WAKE: i32 = 1;

unsafe fn futex_wait(uaddr: *mut AtomicU32, val: u32, timeout: *const libc::timespec) -> i32 {
    unsafe {
        libc::syscall(
            libc::SYS_futex,
            uaddr,
            FUTEX_WAIT,
            val,
            timeout,
            0,
            0,
        ) as i32
    }
}

unsafe fn futex_wake(uaddr: *mut AtomicU32, val: i32) -> i32 {
    unsafe {
        libc::syscall(
            libc::SYS_futex,
            uaddr,
            FUTEX_WAKE,
            val,
            0,
            0,
            0,
        ) as i32
    }
}

#[repr(C)]
struct ShmHeader {
    head: AtomicUsize,
    tail: AtomicUsize,
    futex: AtomicU32,
}

struct ShmRingBuffer {
    fd: File,
    ptr: *mut u8,
}

impl ShmRingBuffer {
    fn new(name: &str) -> io::Result<Self> {
        let c_name = CString::new(name).unwrap();
        let fd = memfd_create(&c_name, MemFdCreateFlag::MFD_CLOEXEC).map_err(|e| io::Error::from_raw_os_error(e as i32))?;
        let file = unsafe { File::from_raw_fd(fd.as_raw_fd()) };
        file.set_len(SHM_SIZE as u64)?;
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                SHM_SIZE,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        } as *mut u8;
        if ptr.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { fd: file, ptr })
    }

    fn header(&self) -> &ShmHeader {
        unsafe { &*(self.ptr as *const ShmHeader) }
    }

    fn buffer(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                self.ptr.offset(std::mem::size_of::<ShmHeader>() as isize),
                SHM_SIZE - std::mem::size_of::<ShmHeader>(),
            )
        }
    }

    fn buffer_mut(&mut self) -> &mut [u8] {
        unsafe {
            std::slice::from_raw_parts_mut(
                self.ptr.offset(std::mem::size_of::<ShmHeader>() as isize),
                SHM_SIZE - std::mem::size_of::<ShmHeader>(),
            )
        }
    }

    pub fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let buffer_len = SHM_SIZE - std::mem::size_of::<ShmHeader>();

        loop {
            let head = self.header().head.load(Ordering::Relaxed);
            let tail = self.header().tail.load(Ordering::Acquire);

            let free_space = if head >= tail {
                buffer_len - (head - tail) - 1 // -1 to distinguish full from empty
            } else {
                tail - head - 1
            };

            if data.len() <= free_space {
                let bytes_to_write = data.len();
                let buffer = self.buffer_mut();

                if head + bytes_to_write > buffer_len {
                    let first_chunk = buffer_len - head;
                    buffer[head..].copy_from_slice(&data[..first_chunk]);
                    buffer[..bytes_to_write - first_chunk].copy_from_slice(&data[first_chunk..]);
                } else {
                    buffer[head..head + bytes_to_write].copy_from_slice(data);
                }

                self.header().head.store((head + bytes_to_write) % buffer_len, Ordering::Release);
                unsafe { futex_wake(&self.header().futex as *const _ as *mut AtomicU32, 1); }
                return Ok(bytes_to_write);
            }

            // Buffer is full, wait
            unsafe {
                futex_wait(&self.header().futex as *const _ as *mut AtomicU32, 0, std::ptr::null());
            }
        }
    }

    pub fn read(&mut self, data: &mut [u8]) -> io::Result<usize> {
        let buffer_len = SHM_SIZE - std::mem::size_of::<ShmHeader>();

        loop {
            let head = self.header().head.load(Ordering::Acquire);
            let tail = self.header().tail.load(Ordering::Relaxed);

            if head == tail {
                // Buffer is empty, wait
                unsafe {
                    futex_wait(&self.header().futex as *const _ as *mut AtomicU32, 0, std::ptr::null());
                }
                continue;
            }

            let available_data = if head >= tail {
                head - tail
            } else {
                buffer_len - (tail - head)
            };

            let bytes_to_read = std::cmp::min(data.len(), available_data);
            let buffer = self.buffer();

            if tail + bytes_to_read > buffer_len {
                let first_chunk = buffer_len - tail;
                data[..first_chunk].copy_from_slice(&buffer[tail..]);
                data[first_chunk..bytes_to_read].copy_from_slice(&buffer[..bytes_to_read - first_chunk]);
            } else {
                data[..bytes_to_read].copy_from_slice(&buffer[tail..tail + bytes_to_read]);
            }

            self.header().tail.store((tail + bytes_to_read) % buffer_len, Ordering::Release);
            unsafe { futex_wake(&self.header().futex as *const _ as *mut AtomicU32, 1); }
            return Ok(bytes_to_read);
        }
    }
}

impl Drop for ShmRingBuffer {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr as *mut libc::c_void, SHM_SIZE);
        }
    }
}

fn main() {
    let mut shm = ShmRingBuffer::new("my_shm").unwrap();

    // Example usage
    let messages = vec![
        b"hello world 1",
        b"hello world 2",
        b"hello world 3",
        b"hello world 4",
        b"hello world 5",
    ];

    for (i, msg) in messages.iter().enumerate() {
        match shm.write(*msg) {
            Ok(bytes_written) => println!("Wrote {} bytes: {}", bytes_written, String::from_utf8_lossy(*msg)),
            Err(e) => eprintln!("Write error {}: {}", i, e),
        }
    }

    for i in 0..messages.len() {
        let mut read_buffer = [0u8; 1024];
        match shm.read(&mut read_buffer) {
            Ok(bytes_read) => {
                println!("Read {} bytes: {}", bytes_read, String::from_utf8_lossy(&read_buffer[..bytes_read]));
            }
            Err(e) => eprintln!("Read error {}: {}", i, e),
        }
    }
}
