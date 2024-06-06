# tftp

A cross-platform tftp server + client written in Rust, trying to be fast and low-footprint.

## Fast and low-footprint

I haven't run any benchmark or similar things to really prove my Tftp app is fast and has an extremely low footprint. But I've put serious thoughts into this and try to achieve that goal by several measures. 
For example, the structs abstracting the Tftp packet types are mostly used zero-copy, i.e. the buffer that is used for reading from socket/file is then directly used by the packet types without any copying/allocation.
In general, allocating is kept to a minimum.

Of course, this all is limited by my current knowledge of Rust. This project is intended to extend my Rust knowledge, improve my coding and become familiar with TFTP, a quite important protocol when working with embedded devices.   

If you think there is room for improvement, please open an issue or a pull request.

## Disclaimer

**This currently needs Rust nightly compiler since I implemented some things with the help of currently non-stabilized features.**

## Supported TFTP features

- [x] Server mode
  - [x] RRQ/GET
  - [x] WRQ/PUT
- [x] Client mode
  - [x] RRQ/GET
  - [x] WRQ/PUT 
- [x] TFTP options
  - [x] Blocksize
  - [x] Timeout
  - [x] Transfer size
  - [ ] Window size

It supports parallel operation with a arbitrary number of peers.

## Supported RFCs
- [x] RFC 1350 - TFTP protocol
- [x] RFC 2347 - TFTP option extension
- [x] RFC 2348 - TFTP blocksize option
- [x] RFC 2349 - TFTP timeout and transfer size options
- [ ] RFC 7440 - TFTP windowsize option

## Big TODOs

- remove need for nightly toolchain
- improve general output, especially for debugging
