# Shared Memory Buffer (charon)

[![Rust](httpshttps://img.shields.io/badge/rust-1.79-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Build Status](https://travis-ci.org/primalpimmy/shmrb-rs.svg?branch=main)](https://travis-ci.org/primalpimmy/shmrb-rs)

A Rust implementation of a Shared Memory Buffer for efficient inter-process communication on Linux.

This project was inspired by the need for a faster alternative to Unix Domain Sockets and TCP for localhost communication.


## Getting Started

This project is currently a test implementation and will be converted into a library soon.

### Prerequisites

*   Rust 1.79 or later
*   Linux

### Installation

1.  Clone the repository:
    ```bash
    git clone https://github.com/primalpimmy/charon.git
    ```
2.  Build the project:
    ```bash
    cd charon
    cargo build --release
    ```

## Usage

```rust
// Example usage will be added here once the library is ready.
```

## Contributing

Contributions are welcome! Please feel free to submit a pull request.

## Acknowledgements

A special thanks to [codelif](https://github.com/codelif) for his implementation of a Shared Memory Stream in Golang, which served as a great inspiration for this project: [shmstream](https://github.com/codelif/shmstream)
