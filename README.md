# tftp

A decent and cross-compatible tftp server + client written in Rust, trying to be fast and low-footprint.

## Really fast and low-footprint??

Many projects on GitHub claim that they are fast, reliable and low-footprint implementations of protocols, etc. So I try to do that too. ;)   
   
Currently, I cannot ensure it is fast and has an extremely low footprint. But I designed it like this with my current knowledge of Rust. This project is intended to extend my Rust knowledge, improve my coding and become familiar with TFTP, a quite important protocol when working with embedded devices.   
I tried to allocate as less as possible, keep the code small and comprehensive while having a good structure, abstraction and making use of Rust's features.   

If you think there is room for improvement, please open an issue or a pull request.

## Supported TFTP features

- [x] Server mode
  - [x] RRQ/GET
  - [x] WRQ/PUT
- [ ] Client mode
  - [ ] RRQ/GET
  - [ ] WRQ/PUT 
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

- ~~WRQ/PUT support - allow the client to store a file on the server~~ - DONE
- ~~Window size option support~~ - NOT PLANNED ATM
- get rid of all the `unwrap`s - they are bad style
- improve general output, especially for debugging
