use std::net::{UdpSocket, SocketAddr, IpAddr};
use std::str::FromStr;
use std::{fmt::Display, time::Duration};
use std::io::{self, Read, Write, BufReader, BufWriter};

pub mod packet;
pub mod options;
pub mod utils;
pub mod error;

pub type Result<T> = std::result::Result<T, ConnectionError>;

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
	error::{ConnectionError, ParseError},
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
	NetAscii,
	Octet,
}
impl Mode {
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
	type Err = ParseError;

	fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
		match &(s.to_ascii_lowercase())[..] {
			consts::TFTP_XFER_MODE_NETASCII => Ok(Self::NetAscii),
			consts::TFTP_XFER_MODE_OCTET => Ok(Self::Octet),
			_ => Err(ParseError::UnknownTxMode)
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
	pub fn new(local_addr: IpAddr, cxl_tok: CancellationToken) -> Result<Self> {
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

	pub fn connect_to(&self, to: SocketAddr) -> Result<()> {
		Ok(self.socket.connect(to)?)
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

	pub fn receive_packet_from<'a>(&self, buf: &'a mut [u8], expect: Option<packet::PacketKind>) -> Result<(packet::TftpPacket<'a>, SocketAddr)> {
		let pkt: packet::TftpPacket;
		
		match self.socket.recv_from(buf) {
			Ok((len, tx)) => {
				pkt = packet::TftpPacket::try_from_buf(&buf[..len])?;

				if let Some(exp_kind) = expect && pkt.packet_kind() != exp_kind {
					return Err(ConnectionError::UnexpectedPacket);
				} else {
					return Ok((pkt, tx));
				}
			}, 
			Err(e) => {
				match e.kind() {
					io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => return Err(ConnectionError::Timeout),
					_ => return Err(e.into()),
				}
			},
		}
	}

	pub fn receive_packet<'a>(&self, buf: &'a mut [u8], expect: Option<packet::PacketKind>) -> Result<packet::TftpPacket<'a>> {
		let recv = self.receive_packet_from(buf, expect)?;
		if let Ok(peer) = self.socket.peer_addr() && peer != recv.1 { /* IP and port must be the same for whole connection */
			self.send_error(ErrorCode::UnknownTid, "").ok();
			return Err(ConnectionError::UnknownTid);
		}

		Ok(recv.0)
	}

	pub fn send_request_to(&self, req: &packet::TftpReq<'_>, to: SocketAddr) -> Result<()> {
		Ok(self.socket.send_to(req.as_bytes(), to).map(|_| ())?)
	}

	pub fn send_packet(&self, pkt: &impl packet::Packet) -> Result<()> {
		Ok(self.socket.send(pkt.as_bytes()).map(|_| ())?)
	}

	pub fn send_and_receive_ack<'a>(&self, data_pkt: &packet::MutableTftpData) -> Result<()> {
		let mut attempts: u8 = 0;
		let mut buf: [u8; 16] = [0; 16];
		loop {
			if self.host_cancelled() {
				return Err(ConnectionError::Cancelled);
			}

			self.send_packet(data_pkt)?;
			match self.receive_packet(&mut buf, Some(packet::PacketKind::Ack)) {
				Ok(reply) => {
					let packet::TftpPacket::Ack(ack) = reply else { panic!() };
					if ack.blocknum() != data_pkt.blocknum() {
						return Err(ConnectionError::UnexpectedBlockAck);
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

	pub fn send_error(&self, code: ErrorCode, msg: &str) -> Result<()> {
		let mut buf: [u8; 64] = [0; 64];
		let err_pkt = TftpErrorBuilder::new()
			.with_buf(&mut buf[..])
			.error_code(code)
			.error_msg(msg)
			.build();

		self.socket.send(err_pkt.as_bytes())?;
		error!("Tftp error: code {}; '{}'", code, msg);
		Ok(())
	}
}

pub async fn receive_data<'a>(conn: TftpConnection, stream: impl Write, init_data: Option<packet::TftpData<'a>>) -> Result<()> {
	let mut buf_write = BufWriter::new(stream);
	let blocksize = conn.opt_blocksize();
	let mut blocknum: u16 = 0;
	let mut data_buf: Vec<u8> = vec![0; 4 + (blocksize as usize)];

	if let Some(first) = init_data {
		buf_write.write_all(first.data())?;
		blocknum += 1;
		
		let ack_pkt = packet::MutableTftpAck::new(blocknum);
		conn.send_packet(&ack_pkt)?;
		if first.data_len() < (blocksize as usize) {
			return Ok(());
		}
	}

	loop {
		if conn.host_cancelled() {
			return Err(ConnectionError::Cancelled)
		}

		let pkt = conn.receive_packet(&mut data_buf[..], Some(packet::PacketKind::Data))?;
		let packet::TftpPacket::Data(pkt) = pkt else { unreachable!() };
		if pkt.blocknum() != blocknum.wrapping_add(1) {
			continue;
		}

		buf_write.write_all(pkt.data())?;
		blocknum = blocknum.wrapping_add(1);
		
		let ack_pkt = packet::MutableTftpAck::new(blocknum);
		conn.send_packet(&ack_pkt)?;
		if pkt.data_len() < (blocksize as usize) {
			break;
		}
	}

	buf_write.flush().ok();
	debug!("received data");
	Ok(())
}

///
/// send_data
/// 
/// This can be used for RRQ in server mode and WRQ in client mode
pub async fn send_data(conn: TftpConnection, stream: impl Read) -> Result<()> {
	if conn.tx_mode() == Mode::NetAscii {
		conn.send_error(ErrorCode::IllegalOperation, "NetAscii mode not supported").ok();
		return Err(ConnectionError::UnsupportedTxMode);
	}

	let blocksize = conn.opt_blocksize();
	let mut buf_read = BufReader::new(stream);
	//debug!("start sending file");

	/* Use only one buffer for file read and packet send. The first 4 bytes are always reserved
	 * for packet header and the file is read after that. */
	let mut read_buf: Vec<u8> = Vec::with_capacity(4 + (blocksize as usize));
	let mut sent_blocks: usize = 0;
	let mut blocknum: u16 = 0;

	read_buf.extend([0; 4]);
	loop {
		if conn.host_cancelled() {
			return Err(ConnectionError::Cancelled);
		}

		let bytes_available = buf_read.by_ref().take(blocksize as u64).read_to_end(&mut read_buf)?;
		let mut pkt = packet::MutableTftpData::try_from(&mut read_buf[..], true).unwrap();
		
		blocknum = blocknum.wrapping_add(1);
		pkt.set_blocknum(blocknum as u16);
		
		conn.send_and_receive_ack(&pkt)?;

		sent_blocks += 1;
		if bytes_available < (blocksize as usize) {
			/* Stop if this was the last block */
			break;
		}
		read_buf.truncate(4);
	}

	debug!("sent file in {} blocks", sent_blocks);
	Ok(())
}
