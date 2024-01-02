use std::collections::HashMap;
use std::ffi::CStr;

use crate::tftp::{consts, RequestKind, Mode};

use super::{utils, options::{TftpOptions, TftpOption}};

pub mod builder;

// ############################################################################
// #### IMMUTABLE PACKETS #####################################################
// ############################################################################

#[derive(Debug)]
pub enum PacketError {
	UnexpectedEof,
	MalformedPacket,
	UnexpectedOpcode,
	InvalidOpcode,
	NotNullTerminated,
	InvalidCharacters,
	UnknownTxMode,
}

pub enum PacketBuf<'a> {
	Borrowed(&'a [u8]),
	Owned(Vec<u8>),
}
impl<'a> PacketBuf<'a> {
	pub fn inner(&'a self) -> &'a [u8] {
		match self {
			PacketBuf::Borrowed(b) => *b,
			PacketBuf::Owned(v) => &v[..]
		}
	} 
}

pub struct TftpReq<'a> {
	buf: PacketBuf<'a>,
}
impl<'a> TftpReq<'a> {
	pub fn from_buf(buf: &'a [u8]) -> Self {
		TftpReq { buf: PacketBuf::Borrowed(buf) }
	}
	fn inner(&self) -> &[u8] {
		match self.buf {
			PacketBuf::Borrowed(ref b) => *b,
			PacketBuf::Owned(ref v) => &v[..],
		}
	}

	pub fn try_from_buf(buf: &'a [u8]) -> Result<Self, PacketError> {
		if buf.len() < 6 {
			return Err(PacketError::UnexpectedEof);
		} else {
			match u16::from_be_bytes([ buf[0], buf[1] ]) {
				consts::OPCODE_RRQ | consts::OPCODE_WRQ => (),
				_ => return Err(PacketError::UnexpectedOpcode),
			}
		}

		Ok(Self::from_buf(buf))
	}

	pub fn kind(&self) -> RequestKind {
		let buf = self.inner();
		match u16::from_be_bytes([ buf[0], buf[1] ]) {
			consts::OPCODE_RRQ => RequestKind::Rrq,
			consts::OPCODE_WRQ => RequestKind::Wrq,
			/* That should never happen, try_from_buf pre-checks the opcode and 
			 * from_buf should only be used when opcode was checked before */
			_ => panic!(),
		}
	}

	pub fn filename(&self) -> Result<&str, PacketError> {
		let buf = self.inner();
		CStr::from_bytes_until_nul(&buf[2..])
			.map_err(|_| PacketError::NotNullTerminated)?
			.to_str()
			.map_err(|_| PacketError::InvalidCharacters)
	}

	pub fn mode(&self) -> Result<Mode, PacketError> {
		let buf = self.inner();
		let mut mode_pos = 0;
		for i in 2..(buf.len() - 1) {
			if buf[i] == 0 {
				mode_pos = i + 1;
				break;
			}
		}

		Mode::try_from(
			CStr::from_bytes_until_nul(&buf[mode_pos..])
				.map_err(|_| PacketError::NotNullTerminated)?
				.to_str()
				.map_err(|_| PacketError::InvalidCharacters)?
		).ok_or(PacketError::UnknownTxMode)
	}

	pub fn options(&self) -> Result<HashMap<&str, &str>, PacketError> {
		let buf = self.inner();
		let mut options: HashMap<&str, &str> = HashMap::new();
		let mut iter = buf[2..].split(|e| *e == 0x00);

		/* skip first two which should be filename + mode */
		iter.advance_by(2).unwrap();
		while let Some(elem) = iter.next() {
			if elem.len() < 2 {
				break;
			}

			let key = std::str::from_utf8(elem)
				.map_err(|_| PacketError::InvalidCharacters)?;
			let Some(value_raw) = iter.next() else { 
				return Err(PacketError::MalformedPacket) 
			};
			let value = std::str::from_utf8(value_raw)
				.map_err(|_| PacketError::InvalidCharacters)?;

			options.insert(key, value);
		}

		Ok(options)
	}
}

pub struct TftpData<'a> { buf: &'a [u8] }
impl <'a> TftpData<'a> {
	pub fn from_buf_unchecked(buf: &'a [u8]) -> Self {
		Self { buf }
	}

	pub fn try_from_buf(buf: &'a [u8]) -> Result<Self, PacketError> {
		if buf.len() < 4 {
			return Err(PacketError::UnexpectedEof);
		}
		match u16::from_be_bytes([ buf[0], buf[1] ]) {
			consts::OPCODE_DATA => (),
			_ => return Err(PacketError::UnexpectedOpcode),
		}

		Ok(Self { buf })
	}

	pub fn blocknum(&self) -> u16 {
		u16::from_be_bytes([ self.buf[2], self.buf[3] ])
	}

	pub fn data(&self) -> &[u8] { &self.buf[4..] }
	pub fn data_len(&self) -> usize { self.buf.len() - 4 }
}

pub struct TftpAck<'a> {
	buf: PacketBuf<'a>
}
impl<'a> TftpAck<'a> {
	fn inner(&self) -> &[u8] {
		match self.buf {
			PacketBuf::Borrowed(ref b) => *b,
			PacketBuf::Owned(ref v) => &v[..],
		}
	}

	pub fn from_buf_unchecked(buf: &'a [u8]) -> Self {
		Self { buf: PacketBuf::Borrowed(buf) }
	}

	pub fn try_from_buf(buf: &'a [u8]) -> Result<Self, PacketError> {
		if buf.len() < 4 {
			return Err(PacketError::UnexpectedEof);
		}
		match u16::from_be_bytes([ buf[0], buf[1] ]) {
			consts::OPCODE_ACK => (),
			_ => return Err(PacketError::UnexpectedOpcode),
		}

		Ok(Self { buf: PacketBuf::Borrowed(buf) })
	}

	pub fn blocknum(&self) -> u16 {
		let buf = self.inner();
		u16::from_be_bytes([ buf[2], buf[3] ])
	}
}

pub struct TftpError<'a> { 
	buf: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum PacketKind {
	Req,
	Data,
	Ack,
	Error,
	OAck,
}

pub enum TftpPacket<'a> {
	Req(TftpReq<'a>),
	Data(TftpData<'a>),
	Ack(TftpAck<'a>),
	Err(TftpError<'a>),
}
impl<'a> TftpPacket<'a> {
	pub fn packet_kind(&self) -> PacketKind {
		match self {
			Self::Req(_) => PacketKind::Req,
			Self::Data(_) => PacketKind::Data,
			Self::Ack(_) => PacketKind::Ack,
			Self::Err(_) => PacketKind::Error,
		}
	}

	pub fn try_from_buf(buf: &'a [u8]) -> Result<Self, PacketError> {
		Ok(
			match u16::from_be_bytes([ buf[0], buf[1] ]) {
				consts::OPCODE_RRQ | consts::OPCODE_WRQ => Self::Req(TftpReq::try_from_buf(buf)?),
				consts::OPCODE_ACK => Self::Ack(TftpAck::try_from_buf(buf)?),
				consts::OPCODE_DATA => Self::Data(TftpData::try_from_buf(buf)?),
				_ => return Err(PacketError::InvalidOpcode),
			}
		)
	}
}

// ############################################################################
// #### MUTABLE PACKETS #######################################################
// ############################################################################

pub enum MutablePacketBuf<'a> {
	Borrowed(&'a mut [u8]),
	Owned(Vec<u8>),
}
impl<'a> MutablePacketBuf<'a> {
	pub fn inner(&'a mut self) -> &'a mut [u8] {
		match self {
			MutablePacketBuf::Borrowed(b) => *b,
			MutablePacketBuf::Owned(v) => &mut v[..]
		}
	} 
}

pub struct MutableTftpReq<'a> {
	buf: MutablePacketBuf<'a>,
}
impl<'a> MutableTftpReq<'a> {
	pub fn buf_as_slice(&'a self) -> &'a [u8] {
		match self.buf {
			MutablePacketBuf::Borrowed(ref b) => *b,
			MutablePacketBuf::Owned(ref b) => (*b).as_slice(),
		}
	}
	pub fn buf_as_slice_mut(&'a mut self) -> &'a mut [u8] {
		match self.buf {
			MutablePacketBuf::Borrowed(ref mut b) => *b,
			MutablePacketBuf::Owned(ref mut b) => (*b).as_mut_slice(),
		}
	}


}

pub struct MutableTftpData<'a> { 
	buf: &'a mut [u8],
	len: usize,
}
impl<'a> MutableTftpData<'a> {

	pub fn try_from(buf: &'a mut [u8], is_filled: bool) -> Result<Self, ()> {
		if buf.len() < 4 {
			return Err(());
		}

		buf[0..=1].copy_from_slice(&consts::OPCODE_DATA.to_be_bytes());

		let buf_len = buf.len();
		Ok(Self { 
			buf,
			len: if is_filled { buf_len } else { 4 }
		})
	}

	/// 
	/// This will panic if the buffer is too small!
	/// 
	pub fn with(buf: &'a mut [u8], blocknum: u16, data: &[u8]) -> Self {
		if buf.len() < (4 + data.len()) {
			panic!();
		}

		let opcode = super::consts::OPCODE_DATA.to_be_bytes();
		let blocknum_bytes = blocknum.to_be_bytes();

		buf[0..=3].copy_from_slice(&[ opcode[0], opcode[1], blocknum_bytes[0], blocknum_bytes[1] ]);
		buf[4..].copy_from_slice(data);
		
		Self { buf, len: 4 + data.len() }
	}

	pub fn set_blocknum(&mut self, blocknum: u16) {
		self.buf[2..=3].copy_from_slice(blocknum.to_be_bytes().as_ref())
	}

	/// 
	/// This will panic if the buffer is too small!
	/// 
	pub fn set_data(&mut self, data: &[u8]) {
		if self.buf.len() < (4 + data.len()) {
			panic!();
		}

		super::utils::copy(data, &mut self.buf[4..]);
		self.len = 4 + data.len();
	}

	pub fn blocknum(&self) -> u16 {
		u16::from_be_bytes([ self.buf[2], self.buf[3] ])
	}
	pub fn len(&self) -> usize { self.len }
	pub fn as_bytes(&self) -> &[u8] { &self.buf[..self.len] }
}

pub struct MutableTftpAck {
	buf: [u8; 4],
}
impl MutableTftpAck {
	pub fn new(blocknum: u16) -> Self {
		let opcode = super::consts::OPCODE_ACK.to_be_bytes();
		let blocknum_b = blocknum.to_be_bytes();
		Self { buf: [ opcode[0], opcode[1], blocknum_b[0], blocknum_b[1] ] }
	}

	pub fn set_blocknum(&mut self, blocknum: u16) {
		self.buf[2..=3].copy_from_slice(blocknum.to_be_bytes().as_ref())
	}

	pub fn as_bytes(&self) -> &[u8] { &self.buf[..] }
}

pub struct MutableTftpOAck { 
	data: Vec<u8>,
	n_options: u8,
}
impl MutableTftpOAck {
	pub fn new() -> Self {
		let opcode = super::consts::OPCODE_OACK.to_be_bytes();
		Self { data: vec![ opcode[0], opcode[1] ], n_options: 0 }
	}

	pub fn with_capacity(capacity: usize) -> Self {
		let mut data: Vec<u8> = Vec::with_capacity(capacity);
		data.extend(super::consts::OPCODE_OACK.to_be_bytes());

		Self { data, n_options: 0 }
	}

	pub fn from(mut buf: Vec<u8>) -> Self {
		buf.resize(2, 0);
		buf.copy_from_slice(&super::consts::OPCODE_OACK.to_be_bytes()[..]);
		Self { data: buf, n_options: 0 }
	}

	pub fn add_option(&mut self, key: &str, val: &str) {
		self.data.extend(key.as_bytes());
		self.data.push(0);
		self.data.extend(val.as_bytes());
		self.data.push(0);
		self.n_options += 1;
	}

	pub fn num_of_options(&self) -> u8 { self.n_options }
	pub fn len(&self) -> usize { self.data.len() }
	pub fn as_bytes(&self) -> &[u8] { &self.data[..] }
}


pub struct MutableTftpError<'a> { 
	buf: &'a mut [u8],
	data_len: usize,
}
impl<'a> MutableTftpError<'a> {
	pub fn with(buf: &'a mut [u8], err_code: super::ErrorCode, err_msg: Option<&str>) -> Result<Self, String> {
		let mut len: usize = 4;
		if buf.len() < (4 + err_msg.unwrap_or("A").len()) {
			return Err(format!("Need larger buffer for a valid ERROR packet!"));
		}

		buf[0..=1].copy_from_slice(&super::consts::OPCODE_ERROR.to_be_bytes()[..]);
		buf[2..=3].copy_from_slice(&(err_code as u16).to_be_bytes()[..]);
		
		if let Some(msg) = err_msg {
			let max_len = buf.len() - 1;
			let copied = super::utils::copy(msg.as_bytes(), &mut buf[4..max_len]);
			buf[4 + copied] = 0;
			len += copied;
		}

		Ok(Self { buf, data_len: len })
	}

	pub fn set_error_code(&mut self, code: super::tftp::ErrorCode) {
		self.buf[2] = 0;
		self.buf[3] = code as u8;
	}

	pub fn set_error_msg(&mut self, msg: &str) {
		let max_len = self.buf.len() - 1;
		let copied = super::utils::copy(msg.as_bytes(), &mut self.buf[4..max_len]);
		self.buf[4 + copied] = 0;
		self.data_len = 4 + copied + 1;
	}

	pub fn len(&self) -> usize { self.buf.len() }
	pub fn as_bytes(&self) -> &[u8] { &self.buf[..self.data_len] }
}

pub enum MutableTftpPacket<'a> {
	Data(MutableTftpData<'a>),
	Ack(MutableTftpAck),
	OAck(MutableTftpOAck),
	Err(MutableTftpError<'a>),
}
impl<'a> MutableTftpPacket<'a> {
	pub fn as_bytes(&self) -> &[u8] {
		match self {
			Self::Data(p) => p.as_bytes(),
			Self::Err(p) => p.as_bytes(),
			Self::OAck(p) => p.as_bytes(),
			Self::Ack(p) => p.as_bytes(),
		}
	}
}