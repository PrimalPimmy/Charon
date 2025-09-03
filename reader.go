package main

import (
	"fmt"
	"os"
	"strconv"
	"syscall"
	"unsafe"
)

const (
	SHM_SIZE = 4096
	FUTEX_WAIT = 0
)

// Must match the layout of the Rust ShmHeader struct
type ShmHeader struct {
	Head  uint64 // AtomicUsize in Rust is 64-bit on x86_64
	Tail  uint64 // AtomicUsize in Rust is 64-bit on x86_64
	Futex uint32 // AtomicU32
	_     uint32 // Padding to align to 64-bit
}

func main() {
	if len(os.Args) < 2 {
		fmt.Println("Usage: go run reader.go <memfd>")
		os.Exit(1)
	}

	fdStr := os.Args[1]
	fd, err := strconv.Atoi(fdStr)
	if err != nil {
		panic(err)
	}

	// Memory map the shared memory file descriptor
	ptr, err := syscall.Mmap(fd, 0, SHM_SIZE, syscall.PROT_READ|syscall.PROT_WRITE, syscall.MAP_SHARED)
	if err != nil {
		panic(err)
	}
	defer syscall.Munmap(ptr)

	header := (*ShmHeader)(unsafe.Pointer(&ptr[0]))
	buffer := ptr[unsafe.Sizeof(ShmHeader{}):]
	bufferLen := SHM_SIZE - int(unsafe.Sizeof(ShmHeader{}))

	for {
		head := header.Head
		tail := header.Tail

		if head == tail {
			fmt.Println("Buffer is empty, waiting...")
			// Atomically check the futex value and wait if it has not changed
			syscall.Syscall6(syscall.SYS_FUTEX, uintptr(unsafe.Pointer(&header.Futex)),
				uintptr(FUTEX_WAIT), uintptr(0), 0, 0, 0)
			continue
		}

		var availableData int
		if head > tail {
			availableData = int(head - tail)
		} else {
			availableData = bufferLen - int(tail-head)
		}

		readBuffer := make([]byte, availableData)
		bytesToRead := availableData

		if int(tail)+bytesToRead > bufferLen {
			firstChunk := bufferLen - int(tail)
			copy(readBuffer, buffer[tail:])
			copy(readBuffer[firstChunk:], buffer[:bytesToRead-firstChunk])
		} else {
			copy(readBuffer, buffer[tail:int(tail)+bytesToRead])
		}

		fmt.Printf("Read %d bytes: %s
", bytesToRead, string(readBuffer))

		// Update the tail pointer
		header.Tail = (tail + uint64(bytesToRead)) % uint64(bufferLen)

		// In a real application, you would likely want to wake up the writer
		// if it's waiting because the buffer was full.
	}
}
