use std::collections::HashMap;
use std::net::{UdpSocket, SocketAddr};
use std::path::PathBuf;
use std::{fmt::Display, time::Duration};
use std::io::{self, BufReader, BufWriter};
use std::fs::File;

use tokio_util::sync::CancellationToken;

pub mod consts {
	pub const TFTP_LISTEN_PORT: u16 = 69;
	pub const DEFAULT_BLOCK_SIZE: u16 = 512;
	pub const DEFAULT_TIMEOUT_SECS: u64 = 5;
	pub const DEFAULT_RETRANSMIT_TRIES: u8 = 3;

	pub const OPT_BLOCKSIZE_IDENT: &str = "blksize";
	pub const OPT_TIMEOUT_IDENT: &str = "timeout";
	pub const OPT_TRANSFERSIZE_IDENT: &str = "tsize";
	pub const OPT_WINDOWSIZE_IDENT: &str = "windowsize";

	pub const OPCODE_RRQ: u16 = 1;
	pub const OPCODE_WRQ: u16 = 2;
	pub const OPCODE_DATA: u16 = 3;
	pub const OPCODE_ACK: u16 = 4;
	pub const OPCODE_ERROR: u16 = 5;
	pub const OPCODE_OACK: u16 = 6;
}

pub mod packet;
pub mod options;
pub mod utils;

use crate::tftp;
use options::*;

// ############################################################################
// ############################################################################
// ############################################################################

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
#[repr(u16)]
pub enum RequestKind {
	Rrq = 0,
	Wrq = 1,
}

#[derive(Debug, Clone, Copy)]
#[repr(u16)]
pub enum ErrorCode {
	NotDefined = 0,
	FileNotFound = 1,
	AccessViolation = 2,
	StorageError = 3,
	IllegalOperation = 4,
	UnknownTid = 5,
	FileExists = 6,
	NoSuchUser = 7,
	InvalidOption = 8,
}
impl Display for ErrorCode {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", *self as u16)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum Mode {
	NetAscii,
	Octet,
}
impl Mode {
	pub fn try_from(input: &str) -> Option<Self> {
		match &(input.to_ascii_lowercase())[..] {
			"netascii" => Some(Self::NetAscii),
			"octet" => Some(Self::Octet),
			_ => None
		}
	}
}
impl Display for Mode {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", match self {
			Self::Octet => "Octet",
			Self::NetAscii => "NetAscii",
		})
	}
}

#[derive(Debug)]
pub enum ReceiveError {
	UnexpectedPacketKind,
	UnexpectedBlockAck,
	Timeout,
	UnknownTid,
	InvalidPacket(packet::PacketError),
	LowerLayer,
}

pub enum BufFile {
	Reader(BufReader<std::fs::File>),
	Writer(BufWriter<std::fs::File>)
}

pub struct TftpConnection {
	req_kind: RequestKind,
	tx_mode: Mode,
	socket: UdpSocket,

	file_path: PathBuf,
	pub file_handle: Option<File>,
	//file: Option<BufFile>,
	options: TftpOptions,

	cxl_tok: CancellationToken,
}

impl TftpConnection {

	#[inline(always)]
	pub fn new(socket: UdpSocket, options: TftpOptions, req_kind: RequestKind, cxl_tok: CancellationToken) -> Self {
		if let Err(_) = socket.peer_addr() {
			/* The socket must be connected already! */
			panic!()
		}

		Self { 
			req_kind,
			socket,
			file_handle: None,
			file_path: PathBuf::default(),
			options,
			cxl_tok,
			tx_mode: Mode::Octet
		}
	}

	// ########################################################################
	// ###### GETTER ##########################################################
	// ########################################################################

	#[inline(always)] pub fn request_kind(&self) 		-> RequestKind 	{ self.req_kind }
	#[inline(always)] pub fn tx_mode(&self) 			-> Mode 		{ self.tx_mode }
	#[inline(always)] pub fn file_path(&self) 			-> &PathBuf 	{ &self.file_path }
	#[inline(always)] pub fn opt_blocksize(&self) 		-> u16 			{ self.options.blocksize }
	#[inline(always)] pub fn opt_timeout(&self) 		-> Duration 	{ self.options.timeout }
	#[inline(always)] pub fn opt_transfer_size(&self) 	-> u16 			{ self.options.transfer_size }
	#[inline(always)] pub fn host_cancelled(&self) 		-> bool 		{ self.cxl_tok.is_cancelled() }

	#[inline(always)]
	pub fn peer(&self) -> SocketAddr {
		/* Socket must be connected on creation of this instance! */
		self.socket.peer_addr().unwrap()
	}
	
	#[inline(always)]
	pub fn take_file_handle(&mut self) -> File {
		self.file_handle.take().unwrap()
	}

	// ########################################################################
	// ###### SETTER ##########################################################
	// ########################################################################

	pub fn set_file_path(&mut self, path: &PathBuf) {
		self.file_path = path.clone()
	}

	pub fn set_file_handle(&mut self, file: File) {
		self.file_handle = Some(file);
	}

	pub fn set_reply_timeout(&mut self, timeout: Duration) {
		self.socket.set_nonblocking(false).unwrap();
		self.socket.set_read_timeout(Some(timeout)).unwrap();
		log::debug!("Timeout set to {}ms", timeout.as_millis());
	}

	// ########################################################################
	// ###### ACTIONS #########################################################
	// ########################################################################

	pub fn receive_packet<'a>(&self, buf: &'a mut[u8], expect: Option<packet::PacketKind>) -> Result<packet::TftpPacket<'a>, ReceiveError> {
		let pkt: packet::TftpPacket;
		
		match self.socket.recv_from(buf) {
			Ok((_, tx)) => {
				if tx != self.peer() { /* IP and Port must be the same for whole connection */
					return Err(ReceiveError::UnknownTid);
				}
	
				pkt = packet::TftpPacket::try_from_buf(buf)
					.map_err(|e| ReceiveError::InvalidPacket(e))?;

				if let Some(exp_kind) = expect && pkt.packet_kind() != exp_kind {
					return Err(ReceiveError::UnexpectedPacketKind);
				} else {
					return Ok(pkt);
				}
			}, 
			Err(e) => {
				match e.kind() {
					io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => return Err(ReceiveError::Timeout),
					_ => return Err(ReceiveError::LowerLayer),
				}
			},
		}
	}

	pub fn send_packet(&self, pkt: &packet::MutableTftpPacket<'_>) {
		self.socket.send(pkt.as_bytes()).unwrap();
	}

	/* This doesn't return the received packet! */
	pub fn send_and_wait_for_reply(
		&self,
		pkt: &packet::MutableTftpPacket<'_>,
		expect: packet::PacketKind,
		max_attempts: u8
	) -> Result<(), ReceiveError> {
		let mut attempts: u8 = 0;
		
		loop {
			if self.host_cancelled() {
				return Err(ReceiveError::Timeout);
			}

			let mut buf: [u8; 64] = [0; 64];

			self.send_packet(pkt);
			match self.receive_packet(&mut buf, Some(expect)) {
				Ok(reply) => {
					if let packet::MutableTftpPacket::Data(ref data_pkt) = pkt &&
					   let packet::TftpPacket::Ack(ack_pkt) = reply &&
					   ack_pkt.blocknum() != data_pkt.blocknum()
					{
						return Err(ReceiveError::UnexpectedBlockAck);
					}
					return Ok(());
				},
				Err(e) => {
					if attempts > max_attempts {
						return Err(e);
					}
				}
			}
			attempts += 1;
		}
	}

	pub fn negotiate_options(&mut self, raw_opts: HashMap<&str, &str>) -> Result<(), OptionError> {
		let mut buf: [u8; 16] = [0; 16];

		if raw_opts.is_empty() {
			return Ok(());
		}
		
		let Ok(mut negotiation) = OptionNegotiation::parse_options(raw_opts) else {
			return Err(OptionError::InvalidOption);
		};

		// Set transfer size if client requested it
		if self.req_kind == RequestKind::Rrq {
			if let Some(opt) = negotiation.find_option_mut(TftpOptionKind::TransferSize) &&
			   let TftpOption::TransferSize(tsz) = opt &&
			   *tsz == 0
			{
				/* TBD: Can this fail if we checked before to have read access? */
				let metadata = self.file_handle.as_ref().unwrap().metadata().unwrap();
				*tsz = metadata.len() as u16;
			}
		}

		let oack_pkt = packet::MutableTftpPacket::OAck(negotiation.build_oack_packet());
		self.send_packet(&oack_pkt);

		match self.receive_packet(&mut buf[..], Some(packet::PacketKind::Ack)) {
			Ok(_) => (),
			Err(_) => return Err(OptionError::NoAck),
		}
		
		self.options.merge_from(&negotiation);
		Ok(())
	}

	pub fn drop(self) { }

	pub fn drop_with_err(self, code: ErrorCode, msg: Option<String>) {
		let mut buf: [u8; 64] = [0; 64];
		let err_pkt = packet::MutableTftpError::with(
			&mut buf,
			code,
			msg.as_deref()
		).unwrap();

		let _ = self.socket.send(err_pkt.as_bytes());

		return log::info!("Tftp error: code {}; {}", code, msg.unwrap_or("".to_string()));
	}
}