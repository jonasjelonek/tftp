use std::net::{UdpSocket, SocketAddr, IpAddr};
use std::path::PathBuf;
use std::{fmt::Display, time::Duration};
use std::io::{self, Read, Write, BufReader, BufWriter};
use std::fs::File;

use tokio_util::sync::CancellationToken;
use log::{info, warn, error, debug, trace};

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

	pub const EMPTY_CHUNK: &[u8] = &[];
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

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, clap::ValueEnum)]
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
	tx_mode: Mode,
	socket: UdpSocket,

	options: TftpOptions,

	cxl_tok: CancellationToken,
}

impl TftpConnection {

	#[inline(always)]
	pub fn new(local_addr: IpAddr, to: SocketAddr, options: TftpOptions, cxl_tok: CancellationToken) -> io::Result<Self> {
		let socket = UdpSocket::bind(SocketAddr::new(local_addr, 0))?;
		socket.connect(to)?;

		Ok(Self {
			socket,
			options,
			cxl_tok,
			tx_mode: Mode::Octet
		})
	}

	// ########################################################################
	// ###### GETTER ##########################################################
	// ########################################################################

	#[inline(always)] pub fn tx_mode(&self) 			-> Mode 		{ self.tx_mode }
	#[inline(always)] pub fn opt_blocksize(&self) 		-> u16 			{ self.options.blocksize }
	#[inline(always)] pub fn opt_timeout(&self) 		-> Duration 	{ self.options.timeout }
	#[inline(always)] pub fn opt_transfer_size(&self) 	-> u32 			{ self.options.transfer_size }
	#[inline(always)] pub fn host_cancelled(&self) 		-> bool 		{ self.cxl_tok.is_cancelled() }

	#[inline(always)]
	pub fn peer(&self) -> SocketAddr {
		self.socket.peer_addr().unwrap()
	}

	#[inline(always)]
	pub fn options_mut(&mut self) -> &mut TftpOptions { &mut self.options }

	// ########################################################################
	// ###### SETTER ##########################################################
	// ########################################################################

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
			Ok((len, tx)) => {
				if tx != self.peer() { /* IP and port must be the same for whole connection */
					return Err(ReceiveError::UnknownTid);
				}
	
				pkt = packet::TftpPacket::try_from_buf(&buf[..len])
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

	pub fn drop(self) { }

	pub fn drop_with_err(self, code: ErrorCode, msg: Option<String>) {
		let mut buf: [u8; 64] = [0; 64];
		let err_pkt = packet::MutableTftpError::with(
			&mut buf,
			code,
			msg.as_deref()
		).unwrap();

		let _ = self.socket.send(err_pkt.as_bytes());

		return info!("Tftp error: code {}; {}", code, msg.unwrap_or("".to_string()));
	}
}

pub async fn receive_file(conn: TftpConnection, file: File) {
	//info!("WRQ from {} to receive '{}'", peer, filename.to_string_lossy());
	//if mode == Mode::NetAscii {
	//	return Err(tftp_err!(IllegalOperation, Some(format!("NetAscii not supported"))));
	//}
	let mut file = BufWriter::new(file);
	let blocksize = conn.opt_blocksize();
	let mut blocknum: u16 = 0;
	let mut data_buf: Vec<u8> = vec![0; 4 + (blocksize as usize)];

	loop {
		if conn.host_cancelled() {
			return conn.drop();
		}

		let pkt = match conn.receive_packet(&mut data_buf[..], Some(packet::PacketKind::Data)) {
			Ok(p) => p,
			Err(e) => {
				error!("Interrupted file transfer: {:?}", e);
				return conn.drop();
			},
		};
		let packet::TftpPacket::Data(pkt) = pkt else { panic!() };
		if pkt.blocknum() != blocknum.wrapping_add(1) {
			continue;
		}

		if let Err(e) = file.write_all(pkt.data()) {
			error!("failed to write to file due to lower layer error: {}", e);
			return conn.drop_with_err(ErrorCode::StorageError, None);
		}

		blocknum = blocknum.wrapping_add(1);
		
		let inner_ack = packet::MutableTftpAck::new(blocknum);
		let ack_pkt = packet::MutableTftpPacket::Ack(inner_ack);
		conn.send_packet(&ack_pkt);

		if pkt.data_len() < (blocksize as usize) {
			break;
		}
	}
	debug!("received file")
}

///
/// send_file
/// 
/// It should be possible to use this for RRQ in server mode and WRQ in client mode
pub async fn send_file(conn: TftpConnection, file: File) {
	let filesize = file.metadata().unwrap().len();
	let blocksize = conn.opt_blocksize();

	if conn.tx_mode() == Mode::NetAscii {
		return conn.drop_with_err(
			tftp::ErrorCode::IllegalOperation, 
			Some(format!("NetAscii not supported"))
		);
	}

	let mut file_read = BufReader::new(file);
	debug!("start sending file");

	/* Use only one buffer for file read and packet send. The first 4 bytes are always reserved
	 * for packet header and the file is read after that. */
	let mut file_buf: Vec<u8> = vec![0; 4 + (blocksize as usize)];
	let mut n_blocks = filesize / (blocksize as u64);
	let remainder = filesize % (blocksize as u64);
	if remainder != 0 {
		/* +1 for remaining bytes */
		n_blocks += 1;
	}

	for blocknum in 1..=n_blocks {
		if conn.host_cancelled() {
			return conn.drop();
		}

		file_buf.truncate(4);
		if let Err(e) = file_read.by_ref().take(blocksize as u64).read_to_end(&mut file_buf) {

		}
		trace!("file_buf has len {} and capacity {}", file_buf.len(), file_buf.capacity());

		let mut pkt = packet::MutableTftpData::try_from(&mut file_buf[..], true).unwrap();
		pkt.set_blocknum(blocknum as u16);
		
		let pkt_ref = packet::MutableTftpPacket::Data(pkt);
		if let Err(_) = conn.send_and_wait_for_reply(&pkt_ref, packet::PacketKind::Ack, 5) {
			return conn.drop();
		}
	}

	/* send a final empty packet in case file_len mod blocksize == 0 */
	if remainder == 0 {
		file_buf.truncate(4);
		
		let mut pkt = packet::MutableTftpData::try_from(&mut file_buf[..], false).unwrap();
		pkt.set_blocknum(pkt.blocknum() + 1);
		pkt.set_data(consts::EMPTY_CHUNK);
		
		let pkt_ref = packet::MutableTftpPacket::Data(pkt);
		match conn.send_and_wait_for_reply(&pkt_ref, packet::PacketKind::Ack, 5) {
			Ok(_) => (),
			Err(_e) => return conn.drop(),
		}
	}

	debug!("sent file");
}