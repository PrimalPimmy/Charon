use charon::ShmRingBuffer;
use std::io;

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
            Ok(bytes_written) => println!(
                "Wrote {} bytes: {}",
                bytes_written,
                String::from_utf8_lossy(*msg)
            ),
            Err(e) => eprintln!("Write error {}: {}", i, e),
        }
    }

    for i in 0..messages.len() {
        let mut read_buffer = [0u8; 1024];
        match shm.read(&mut read_buffer) {
            Ok(bytes_read) => {
                println!(
                    "Read {} bytes: {}",
                    bytes_read,
                    String::from_utf8_lossy(&read_buffer[..bytes_read])
                );
            }
            Err(e) => eprintln!("Read error {}: {}", i, e),
        }
    }
}