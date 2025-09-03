
use linux_futex::{AsFutex, Futex, Private};
use nix::sys::memfd::{MemFdCreateFlag, memfd_create};
use nix::sys::mman::{MapFlags, ProtFlags, mmap, munmap};
use std::ffi::CString;
use std::io::{self};
use std::num::NonZero;
use std::os::fd::AsRawFd;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

const SHM_SIZE: usize = 4096;

#[repr(C)]
struct ShmHeader {
    head: AtomicUsize,
    tail: AtomicUsize,
    futex: AtomicU32,
}

use nix::unistd::close;
use nix::unistd::ftruncate;

pub struct ShmRingBuffer {
    ptr: NonNull<u8>,
    fd: i32,
}

impl ShmRingBuffer {
    pub fn new(name: &str) -> io::Result<Self> {
        let c_name = CString::new(name).unwrap();
        let fd = memfd_create(&c_name, MemFdCreateFlag::MFD_CLOEXEC)
            .map_err(|e| io::Error::from_raw_os_error(e as i32))?;
        ftruncate(&fd, SHM_SIZE as i64).map_err(|e| io::Error::from_raw_os_error(e as i32))?;
        let ptr = unsafe {
            mmap(
                None,
                NonZero::new(SHM_SIZE).unwrap(),
                ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                MapFlags::MAP_SHARED,
                &fd,
                0,
            )
        }
        .map_err(|e| io::Error::from_raw_os_error(e as i32))?;
        Ok(Self {
            ptr: ptr.cast(),
            fd: fd.as_raw_fd(),
        })
    }

    fn header(&self) -> &ShmHeader {
        unsafe { &*(self.ptr.as_ptr() as *const ShmHeader) }
    }

    fn futex(&self) -> &Futex<Private> {
        self.header().futex.as_futex()
    }

    fn buffer(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                self.ptr
                    .as_ptr()
                    .offset(std::mem::size_of::<ShmHeader>() as isize),
                SHM_SIZE - std::mem::size_of::<ShmHeader>(),
            )
        }
    }

    fn buffer_mut(&mut self) -> &mut [u8] {
        unsafe {
            std::slice::from_raw_parts_mut(
                self.ptr
                    .as_ptr()
                    .offset(std::mem::size_of::<ShmHeader>() as isize),
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

                self.header()
                    .head
                    .store((head + bytes_to_write) % buffer_len, Ordering::Release);
                let _ = self.futex().wake(1);
                return Ok(bytes_to_write);
            }

            // Buffer is full, wait
            let _ = self
                .futex()
                .wait(self.header().futex.load(Ordering::Relaxed));
        }
    }

    pub fn read(&mut self, data: &mut [u8]) -> io::Result<usize> {
        let buffer_len = SHM_SIZE - std::mem::size_of::<ShmHeader>();

        loop {
            let head = self.header().head.load(Ordering::Acquire);
            let tail = self.header().tail.load(Ordering::Relaxed);

            if head == tail {
                // Buffer is empty, wait
                let _ = self
                    .futex()
                    .wait(self.header().futex.load(Ordering::Relaxed));
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
                data[first_chunk..bytes_to_read]
                    .copy_from_slice(&buffer[..bytes_to_read - first_chunk]);
            } else {
                data[..bytes_to_read].copy_from_slice(&buffer[tail..tail + bytes_to_read]);
            }

            self.header()
                .tail
                .store((tail + bytes_to_read) % buffer_len, Ordering::Release);
            let _ = self.futex().wake(1);
            return Ok(bytes_to_read);
        }
    }
}

impl Drop for ShmRingBuffer {
    fn drop(&mut self) {
        unsafe {
            let _ = munmap(self.ptr.cast(), SHM_SIZE);
        }
        let _ = close(self.fd);
    }
}


