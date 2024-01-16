use std::net::{UdpSocket, SocketAddr, IpAddr};
use std::str::FromStr;
use std::{fmt::Display, time::Duration};
use std::io::{self, Read, Write, BufReader, BufWriter};
use std::fs::File;

pub mod packet;
pub mod options;
pub mod utils;
pub mod error;

#[allow(unused)]
use log::{info, warn, error, debug, trace};
use tokio_util::sync::CancellationToken;

pub mod consts {
	pub const TFTP_LISTEN_PORT: u16 = 69;
	pub const DEFAULT_BLOCK_SIZE: u16 = 512;
	pub const DEFAULT_TIMEOUT_SECS: u8 = 5;
	pub const DEFAULT_RETRANSMIT_ATTEMPTS: u8 = 5;

	pub const TFTP_XFER_MODE_OCTET: &str = "octet";
	pub const TFTP_XFER_MODE_NETASCII: &str = "netascii";

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

use crate::tftp::{
	packet::builder::TftpErrorBuilder,
	packet::Packet,
	error::ErrorCode,
};
use options::*;


// ############################################################################
// ############################################################################
// ############################################################################

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u16)]
pub enum RequestKind {
	Rrq = 1,
	Wrq = 2,
}
impl Display for RequestKind {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Rrq => write!(f, "RRQ"),
			Self::Wrq => write!(f, "WRQ"),
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, clap::ValueEnum)]
pub enum Mode {
	NetAscii,
	Octet,
}
impl Mode {
	pub fn try_from(input: &str) -> Option<Self> {
		match &(input.to_ascii_lowercase())[..] {
			consts::TFTP_XFER_MODE_NETASCII => Some(Self::NetAscii),
			consts::TFTP_XFER_MODE_OCTET => Some(Self::Octet),
			_ => None
		}
	}
	pub fn as_str(&self) -> &'static str {
		match self {
			Self::Octet => consts::TFTP_XFER_MODE_OCTET,
			Self::NetAscii => consts::TFTP_XFER_MODE_NETASCII,
		}
	}
}
impl Display for Mode {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.as_str())
	}
}
impl FromStr for Mode {
	type Err = error::ParseModeError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match &(s.to_ascii_lowercase())[..] {
			consts::TFTP_XFER_MODE_NETASCII => Ok(Self::NetAscii),
			consts::TFTP_XFER_MODE_OCTET => Ok(Self::Octet),
			_ => Err(error::ParseModeError)
		}
	}
}

#[derive(Debug)]
pub enum ReceiveError {
	UnexpectedPacketKind,
	UnexpectedBlockAck,
	Timeout,
	UnknownTid,
	InvalidPacket(error::PacketError),
	LowerLayer(io::Error),
}
impl Display for ReceiveError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::UnknownTid => write!(f, "Unknown or unexpected TID"),
			Self::Timeout => write!(f, "Timeout"),
			Self::UnexpectedPacketKind => write!(f, "Unexpected kind of packet"),
			Self::UnexpectedBlockAck => write!(f, "ACK for unexpected block number"),
			Self::InvalidPacket(e) => write!(f, "Invalid packet ({})", e),
			Self::LowerLayer(e) => write!(f, "LowerLayer error: {}", e),
		}
	}
}

pub struct TftpConnection {
	tx_mode: Mode,
	socket: UdpSocket,

	options: TftpOptions,
	cxl_tok: CancellationToken,
}

impl TftpConnection {

	#[inline(always)]
	pub fn new(local_addr: IpAddr, cxl_tok: CancellationToken) -> io::Result<Self> {
		let socket = UdpSocket::bind(SocketAddr::new(local_addr, 0))?;

		let mut conn = Self {
			socket,
			options: TftpOptions::default(),
			cxl_tok,
			tx_mode: Mode::Octet
		};
		conn.set_reply_timeout(conn.opt_timeout());
		Ok(conn)
	}

	// ########################################################################
	// ###### GETTER ##########################################################
	// ########################################################################

	#[inline(always)] pub fn tx_mode(&self) 			-> Mode 		{ self.tx_mode }
	#[inline(always)] pub fn opt_blocksize(&self) 		-> u16 			{ self.options.blocksize }
	#[inline(always)] pub fn opt_timeout(&self) 		-> Duration 	{ self.options.timeout }
	#[inline(always)] pub fn opt_transfer_size(&self) 	-> u32 			{ self.options.transfer_size }
	#[inline(always)] pub fn host_cancelled(&self) 		-> bool 		{ self.cxl_tok.is_cancelled() }
	#[inline(always)] pub fn peer(&self)				-> SocketAddr	{ self.socket.peer_addr().unwrap() }

	// ########################################################################
	// ###### SETTER ##########################################################
	// ########################################################################

	pub fn connect_to(&self, to: SocketAddr) -> io::Result<()> {
		self.socket.connect(to)
	}

	pub fn set_reply_timeout(&mut self, timeout: Duration) {
		self.socket.set_nonblocking(false).unwrap();
		self.socket.set_read_timeout(Some(timeout)).unwrap();
		debug!("Timeout set to {}ms", timeout.as_millis());
	}

	pub fn set_tx_mode(&mut self, tx_mode: Mode) { self.tx_mode = tx_mode }

	pub fn set_options(&mut self, opts: &[TftpOption]) {
		for opt in opts {
			match opt {
				TftpOption::Blocksize(bs) => self.options.blocksize = *bs,
				TftpOption::Timeout(t) => self.options.timeout = *t,
				TftpOption::TransferSize(ts) => self.options.transfer_size = *ts,
			}
		}

		self.set_reply_timeout(self.opt_timeout());
	}

	// ########################################################################
	// ###### ACTIONS #########################################################
	// ########################################################################

	pub fn receive_packet_from<'a>(&self, buf: &'a mut [u8], expect: Option<packet::PacketKind>) -> Result<(packet::TftpPacket<'a>, SocketAddr), ReceiveError> {
		let pkt: packet::TftpPacket;
		
		match self.socket.recv_from(buf) {
			Ok((len, tx)) => {
				pkt = packet::TftpPacket::try_from_buf(&buf[..len])
					.map_err(|e| ReceiveError::InvalidPacket(e))?;

				if let Some(exp_kind) = expect && pkt.packet_kind() != exp_kind {
					return Err(ReceiveError::UnexpectedPacketKind);
				} else {
					return Ok((pkt, tx));
				}
			}, 
			Err(e) => {
				match e.kind() {
					io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => return Err(ReceiveError::Timeout),
					_ => return Err(ReceiveError::LowerLayer(e)),
				}
			},
		}
	}

	pub fn receive_packet<'a>(&self, buf: &'a mut [u8], expect: Option<packet::PacketKind>) -> Result<packet::TftpPacket<'a>, ReceiveError> {
		let recv = self.receive_packet_from(buf, expect)?;
		if let Ok(peer) = self.socket.peer_addr() && peer != recv.1 { /* IP and port must be the same for whole connection */
			return Err(ReceiveError::UnknownTid);
		}

		Ok(recv.0)
	}

	pub fn send_request_to(&self, req: &packet::TftpReq<'_>, to: SocketAddr) -> io::Result<()> {
		self.socket.send_to(req.as_bytes(), to).map(|_| ())
	}

	pub fn send_packet(&self, pkt: &impl packet::Packet) -> io::Result<()> {
		self.socket.send(pkt.as_bytes()).map(|_| ())
	}

	pub fn send_and_receive_ack<'a>(&self, data_pkt: &packet::MutableTftpData) -> Result<(), ReceiveError> {
		let mut attempts: u8 = 0;
		let mut buf: [u8; 16] = [0; 16];
		loop {
			if self.host_cancelled() {
				return Err(ReceiveError::Timeout);
			}

			self.send_packet(data_pkt).map_err(|e| ReceiveError::LowerLayer(e))?;
			match self.receive_packet(&mut buf, Some(packet::PacketKind::Ack)) {
				Ok(reply) => {
					let packet::TftpPacket::Ack(ack) = reply else { panic!() };
					if ack.blocknum() != data_pkt.blocknum() {
						return Err(ReceiveError::UnexpectedBlockAck);
					}
					return Ok(())
				},
				Err(e) => {
					if attempts > consts::DEFAULT_RETRANSMIT_ATTEMPTS {
						return Err(e);
					}
					attempts += 1;
				}
			}
		}
	}

	/* This could be used when Rust's new borrow checker is stable/usable. The current one
	 * complains about recv_buf being multiple times borrowed multiple times. However, when
	 * looking at the code it should be perfectly fine to do it that way. With
	 * RUSTC_FLAGS="-Z polonius" (to run the current implementation of the new borrow checker)
	 * this compiles just fine. */
	/* pub fn send_and_wait_for_reply<'a>(
		&self,
		tx_pkt: &impl packet::Packet,
		recv_buf: &'a mut [u8],
		expect: packet::PacketKind
	) -> Result<TftpPacket<'a>, ReceiveError> {
		let mut attempts: u8 = 0;
		//let mut buf: [u8; 64] = [0; 64];
		loop {
			if self.host_cancelled() {
				return Err(ReceiveError::Timeout);
			}

			self.send_packet(tx_pkt);
			match self.receive_packet(recv_buf, Some(expect)) {
				Ok(reply) => return Ok(reply),
				Err(e) => {
					if attempts > self.retry_attempts {
						return Err(e);
					}
				}
			}
			attempts += 1;
		}
	} */

	pub fn send_error(&self, code: ErrorCode, msg: &str) {
		let mut buf: [u8; 64] = [0; 64];
		let err_pkt = TftpErrorBuilder::new()
			.with_buf(&mut buf[..])
			.error_code(code)
			.error_msg(msg)
			.build();

		let _ = self.socket.send(err_pkt.as_bytes());
		error!("Tftp error: code {}; '{}'", code, msg);
	}

	pub fn drop(self) { }

	pub fn drop_with_err(self, code: ErrorCode, msg: &str) {
		self.send_error(code, msg);
		return;
	}
}

pub async fn receive_file<'a>(conn: TftpConnection, file: File, init_data: Option<packet::TftpData<'a>>) {
	let mut file = BufWriter::new(file);
	let blocksize = conn.opt_blocksize();
	let mut blocknum: u16 = 0;
	let mut data_buf: Vec<u8> = vec![0; 4 + (blocksize as usize)];

	if let Some(first) = init_data {
		if let Err(e) = file.write_all(first.data()) {
			error!("failed to write to file due to lower layer error: {}", e);
			return conn.drop_with_err(ErrorCode::StorageError, "");
		}

		blocknum = blocknum.wrapping_add(1);
		
		let ack_pkt = packet::MutableTftpAck::new(blocknum);
		let _ = conn.send_packet(&ack_pkt);
		if first.data_len() < (blocksize as usize) {
			return;
		}
	}

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
		let packet::TftpPacket::Data(pkt) = pkt else { unreachable!() };
		if pkt.blocknum() != blocknum.wrapping_add(1) {
			continue;
		}

		if let Err(e) = file.write_all(pkt.data()) {
			error!("failed to write to file due to lower layer error: {}", e);
			return conn.drop_with_err(ErrorCode::StorageError, "");
		}

		blocknum = blocknum.wrapping_add(1);
		
		let ack_pkt = packet::MutableTftpAck::new(blocknum);
		let _ = conn.send_packet(&ack_pkt);
		if pkt.data_len() < (blocksize as usize) {
			break;
		}
	}

	let _ = file.flush();
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
		return conn.drop_with_err(ErrorCode::IllegalOperation, "NetAscii mode not supported");
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
			error!("send_file interrupted: {}", e);
			return conn.drop_with_err(ErrorCode::StorageError, "");
		}
		trace!("file_buf has len {} and capacity {}", file_buf.len(), file_buf.capacity());

		let mut pkt = packet::MutableTftpData::try_from(&mut file_buf[..], true).unwrap();
		pkt.set_blocknum(blocknum as u16);
		
		if let Err(e) = conn.send_and_receive_ack(&pkt) {
			error!("send_file interrupted: {}", e);
			return conn.drop();
		}
	}

	/* send a final empty packet in case file_len mod blocksize == 0 */
	if remainder == 0 {
		file_buf.truncate(4);
		
		let mut pkt = packet::MutableTftpData::try_from(&mut file_buf[..], false).unwrap();
		pkt.set_blocknum(pkt.blocknum() + 1);
		pkt.set_data(consts::EMPTY_CHUNK);
		
		if let Err(e) = conn.send_and_receive_ack(&pkt) {
			error!("send_file interrupted: {}", e);
			return conn.drop();
		}
	}

	debug!("sent file");
}
