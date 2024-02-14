use std::collections::HashMap;
use std::fmt::Display;
use std::ffi::CStr;

use crate::tftp::{
	consts,
	error::{ErrorCode, ParseError},
	Mode,
	RequestKind,
	utils
};

pub mod builder;

pub type Result<T> = std::result::Result<T, ParseError>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PacketKind {
	Req(RequestKind),
	Data,
	Ack,
	Error,
	OAck,
}
impl Display for PacketKind {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Req(ref r) => write!(f, "REQ ({})", *r),
			Self::Ack => write!(f, "ACK"),
			Self::Data => write!(f, "DATA"),
			Self::OAck => write!(f, "OACK"),
			Self::Error => write!(f, "ERROR"),
		}
	}
}

pub trait Packet {
	fn packet_kind(&self) -> PacketKind;
	fn as_bytes(&self) -> &[u8];
}

// ############################################################################
// ############################################################################
// #### IMMUTABLE PACKETS #####################################################
// ############################################################################
// ############################################################################

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

// ############################################################################
// #### TFTP REQUEST (RRQ/WRQ) ################################################
// ############################################################################

pub struct TftpReq<'a> {
	inner: PacketBuf<'a>,
}
impl<'a> TftpReq<'a> {

	/// Creates a TftpReq instance with the given borrowed slice.
	/// 
	/// **Only use this when you made sure the content is valid for a
	/// TftpReq!!**
	#[inline] pub fn from_borrowed(buf: &'a [u8]) -> Self {
		TftpReq { inner: PacketBuf::Borrowed(buf) }
	}

	/// Creates a TftpReq instance with the given owned buffer, consuming
	/// it.
	/// 
	/// **Only use this when you made sure the content is valid for a
	/// TftpReq!!**
	#[inline] pub fn from_owned(buf: Vec<u8>) -> Self {
		TftpReq { inner: PacketBuf::Owned(buf) }
	}

	fn inner(&self) -> &[u8] {
		match self.inner {
			PacketBuf::Borrowed(ref b) => *b,
			PacketBuf::Owned(ref v) => &v[..],
		}
	}

	/// Checks if the given slice contains valid TftpReq content.
	/// 
	/// *This is used by the TryFrom<> trait implementations.*
	fn check_from_slice(buf: &'a [u8]) -> Result<()> {
		if buf.len() < 6 {
			Err(ParseError::UnexpectedEof)
		} else {
			match u16::from_be_bytes([ buf[0], buf[1] ]) {
				consts::OPCODE_RRQ | consts::OPCODE_WRQ => Ok(()),
				_ => Err(ParseError::UnexpectedOpcode),
			}
		}
	}

	pub fn kind(&self) -> RequestKind {
		let buf = self.inner();
		match u16::from_be_bytes([ buf[0], buf[1] ]) {
			consts::OPCODE_RRQ => RequestKind::Rrq,
			consts::OPCODE_WRQ => RequestKind::Wrq,
			_ => unreachable!(),
		}
	}

	pub fn filename(&self) -> Result<&str> {
		let buf = self.inner();
		Ok(CStr::from_bytes_until_nul(&buf[2..])?.to_str()?)
	}

	pub fn mode(&self) -> Result<Mode> {
		let buf = self.inner();
		let mut mode_pos = 0;
		for i in 2..(buf.len() - 1) {
			if buf[i] == 0 {
				mode_pos = i + 1;
				break;
			}
		}

		Ok(CStr::from_bytes_until_nul(&buf[mode_pos..])?
			.to_str()?
			.parse()?
		)
	}

	pub fn options(&self) -> Result<HashMap<&str, &str>> {
		let buf = self.inner();
		let mut options: HashMap<&str, &str> = HashMap::new();
		let mut iter = buf[2..].split(|e| *e == 0x00);

		/* skip first two which should be filename + mode */
		let _ = iter.nth(1); /* could be replaced by advance_by(2) when stabilized to be more intuitive */
		while let Some(elem) = iter.next() {
			if elem.len() < 2 {
				break;
			}

			let key = std::str::from_utf8(elem)?;
			let Some(value_raw) = iter.next() else { 
				return Err(ParseError::MalformedPacket) 
			};
			let value = std::str::from_utf8(value_raw)?;
			options.insert(key, value);
		}

		Ok(options)
	}
}
impl<'a> Packet for TftpReq<'a> {
	fn packet_kind(&self) -> PacketKind { PacketKind::Req(self.kind()) }
	fn as_bytes(&self) -> &[u8] { self.inner() }
}
impl<'a> TryFrom<&'a [u8]> for TftpReq<'a> {
	type Error = ParseError;

	fn try_from(buf: &'a [u8]) -> Result<Self> {
		TftpReq::check_from_slice(buf)?;
		Ok(Self::from_borrowed(buf))
	}
}
impl TryFrom<Vec<u8>> for TftpReq <'_> {
	type Error = ParseError;

	fn try_from(vec: Vec<u8>) -> Result<Self> {
		TftpReq::check_from_slice(&vec[..])?;
		Ok(Self::from_owned(vec))
	}
}

// ############################################################################
// #### TFTP DATA #############################################################
// ############################################################################

pub struct TftpData<'a> { 
	inner: PacketBuf<'a>
}
impl <'a> TftpData<'a> {
	#[inline] pub fn from_borrowed(buf: &'a [u8]) -> Self {
		Self { inner: PacketBuf::Borrowed(buf) }
	}
	#[inline] pub fn from_owned(buf: Vec<u8>) -> Self {
		Self { inner: PacketBuf::Owned(buf) }
	}

	fn inner(&self) -> &[u8] {
		match self.inner {
			PacketBuf::Borrowed(ref b) => *b,
			PacketBuf::Owned(ref v) => &v[..],
		}
	}

	fn check_from_slice(buf: &'a [u8]) -> Result<()> {
		if buf.len() < 4 {
			return Err(ParseError::UnexpectedEof);
		}
		match u16::from_be_bytes([ buf[0], buf[1] ]) {
			consts::OPCODE_DATA => (),
			_ => return Err(ParseError::UnexpectedOpcode),
		}
		Ok(())
	}

	#[inline] pub fn blocknum(&self) -> u16 {
		let buf = self.inner();
		u16::from_be_bytes([ buf[2], buf[3] ])
	}

	#[inline] pub fn data(&self) -> &[u8] { &self.inner()[4..] }
	#[inline] pub fn data_len(&self) -> usize { self.inner().len() - 4 }
}
impl<'a> Packet for TftpData<'a> {
	fn packet_kind(&self) -> PacketKind { PacketKind::Data }
	fn as_bytes(&self) -> &[u8] { self.inner() }
}
impl<'a> TryFrom<&'a [u8]> for TftpData<'a> {
	type Error = ParseError;

	fn try_from(buf: &'a [u8]) -> Result<Self> {
		TftpData::check_from_slice(buf)?;
		Ok(Self::from_borrowed(buf))
	}
}
impl TryFrom<Vec<u8>> for TftpData<'_> {
	type Error = ParseError;

	fn try_from(vec: Vec<u8>) -> Result<Self> {
		TftpData::check_from_slice(&vec[..])?;
		Ok(Self::from_owned(vec))
	}
}

// ############################################################################
// #### TFTP ACK ##############################################################
// ############################################################################

pub struct TftpAck<'a> {
	inner: PacketBuf<'a>
}
impl<'a> TftpAck<'a> {
	#[inline] pub fn from_borrowed(buf: &'a [u8]) -> Self {
		Self { inner: PacketBuf::Borrowed(buf) }
	}
	#[inline] pub fn from_owned(vec: Vec<u8>) -> Self {
		Self { inner: PacketBuf::Owned(vec) }
	}
	
	fn inner(&self) -> &[u8] {
		match self.inner {
			PacketBuf::Borrowed(ref b) => *b,
			PacketBuf::Owned(ref v) => &v[..],
		}
	}

	pub fn check_from_slice(buf: &'a [u8]) -> Result<()> {
		if buf.len() < 4 {
			return Err(ParseError::UnexpectedEof);
		}
		match u16::from_be_bytes([ buf[0], buf[1] ]) {
			consts::OPCODE_ACK => (),
			_ => return Err(ParseError::UnexpectedOpcode),
		}
		Ok(())
	}

	pub fn blocknum(&self) -> u16 {
		let buf = self.inner();
		u16::from_be_bytes([ buf[2], buf[3] ])
	}
}
impl<'a> Packet for TftpAck<'a> {
	fn packet_kind(&self) -> PacketKind { PacketKind::Ack }
	fn as_bytes(&self) -> &[u8] { self.inner() }
}
impl<'a> TryFrom<&'a [u8]> for TftpAck<'a> {
	type Error = ParseError;

	fn try_from(buf: &'a [u8]) -> Result<Self> {
		TftpAck::check_from_slice(buf)?;
		Ok(Self::from_borrowed(buf))
	}
}
impl TryFrom<Vec<u8>> for TftpAck<'_> {
	type Error = ParseError;

	fn try_from(vec: Vec<u8>) -> Result<Self> {
		TftpAck::check_from_slice(&vec[..])?;
		Ok(Self::from_owned(vec))
	}
}

// ############################################################################
// #### TFTP OACK #############################################################
// ############################################################################

pub struct TftpOAck<'a> {
	inner: PacketBuf<'a>,
}
impl<'a> TftpOAck<'a> {
	#[inline] pub fn from_borrowed(buf: &'a [u8]) -> Self {
		Self { inner: PacketBuf::Borrowed(buf) }
	}
	#[inline] pub fn from_owned(vec: Vec<u8>) -> Self {
		Self { inner: PacketBuf::Owned(vec) }
	}

	fn inner(&self) -> &[u8] {
		match self.inner {
			PacketBuf::Borrowed(ref b) => *b,
			PacketBuf::Owned(ref v) => &v[..],
		}
	}

	fn check_from_slice(buf: &'a [u8]) -> Result<()> {
		if buf.len() < 6 {
			return Err(ParseError::UnexpectedEof);
		}
		if u16::from_be_bytes([ buf[0], buf[1] ]) != consts::OPCODE_OACK {
			return Err(ParseError::UnexpectedOpcode);
		}
		Ok(())
	}

	pub fn options(&self) -> Result<HashMap<&str, &str>> {
		let buf = self.inner();
		let mut options: HashMap<&str, &str> = HashMap::new();
		let mut iter = buf[2..].split(|e| *e == 0x00);

		while let Some(elem) = iter.next() {
			if elem.len() < 2 {
				break;
			}

			let key = std::str::from_utf8(elem)?;
			let Some(value_raw) = iter.next() else { 
				return Err(ParseError::MalformedPacket) 
			};
			let value = std::str::from_utf8(value_raw)?;

			options.insert(key, value);
		}

		Ok(options)
	}
}
impl<'a> Packet for TftpOAck<'a> {
	#[inline] fn packet_kind(&self) -> PacketKind { PacketKind::OAck }
	#[inline] fn as_bytes(&self) -> &[u8] { self.inner() }
}
impl<'a> TryFrom<&'a [u8]> for TftpOAck<'a> {
	type Error = ParseError;

	fn try_from(buf: &'a [u8]) -> Result<Self> {
		TftpOAck::check_from_slice(buf)?;
		Ok(Self::from_borrowed(buf))
	}
}
impl TryFrom<Vec<u8>> for TftpOAck<'_> {
	type Error = ParseError;

	fn try_from(vec: Vec<u8>) -> Result<Self> {
		TftpOAck::check_from_slice(&vec[..])?;
		Ok(Self::from_owned(vec))
	}
}

// ############################################################################
// #### TFTP ERROR ############################################################
// ############################################################################

pub struct TftpError<'a> { 
	inner: PacketBuf<'a>,
}
impl<'a> TftpError<'a> {
	#[inline] pub fn from_borrowed(buf: &'a [u8]) -> Self {
		Self { inner: PacketBuf::Borrowed(buf) }
	}
	#[inline] pub fn from_owned(vec: Vec<u8>) -> Self {
		Self { inner: PacketBuf::Owned(vec) }
	}

	fn inner(&self) -> &[u8] {
		match self.inner {
			PacketBuf::Borrowed(ref b) => *b,
			PacketBuf::Owned(ref v) => &v[..],
		}
	}

	fn check_from_slice(buf: &'a [u8]) -> Result<()> {
		if buf.len() < 6 {
			return Err(ParseError::UnexpectedEof);
		}
		if u16::from_be_bytes([ buf[0], buf[1] ]) != consts::OPCODE_OACK {
			return Err(ParseError::UnexpectedOpcode);
		}
		Ok(())
	}

	pub fn error_code(&self) -> ErrorCode {
		let buf = self.inner();
		ErrorCode::try_from(u16::from_be_bytes([ buf[2], buf[3] ])).unwrap()
	}

	pub fn error_msg(&'a self) -> &'a str {
		std::str::from_utf8(&self.inner()[4..]).unwrap()
	}
}
impl<'a> Packet for TftpError<'a> {
	#[inline] fn packet_kind(&self) -> PacketKind { PacketKind::Error }
	#[inline] fn as_bytes(&self) -> &[u8] { self.inner() }
}
impl<'a> TryFrom<&'a [u8]> for TftpError<'a> {
	type Error = ParseError;

	fn try_from(buf: &'a [u8]) -> Result<Self> {
		TftpError::check_from_slice(buf)?;
		Ok(Self::from_borrowed(buf))
	}
}
impl TryFrom<Vec<u8>> for TftpError<'_> {
	type Error = ParseError;

	fn try_from(vec: Vec<u8>) -> Result<Self> {
		TftpError::check_from_slice(&vec[..])?;
		Ok(Self::from_owned(vec))
	}
}

pub enum TftpPacket<'a> {
	Req(TftpReq<'a>),
	Data(TftpData<'a>),
	Ack(TftpAck<'a>),
	OAck(TftpOAck<'a>),
	Err(TftpError<'a>),
}
impl<'a> TftpPacket<'a> {
	pub fn packet_kind(&self) -> PacketKind {
		match self {
			Self::Req(rq) => PacketKind::Req(rq.kind()),
			Self::Data(_) => PacketKind::Data,
			Self::Ack(_) => PacketKind::Ack,
			Self::Err(_) => PacketKind::Error,
			Self::OAck(_) => PacketKind::OAck,
		}
	}

	pub fn try_from_buf(buf: &'a [u8]) -> Result<Self> {
		Ok(
			match u16::from_be_bytes([ buf[0], buf[1] ]) {
				consts::OPCODE_RRQ | consts::OPCODE_WRQ => Self::Req(TftpReq::try_from(buf)?),
				consts::OPCODE_ACK => Self::Ack(TftpAck::try_from(buf)?),
				consts::OPCODE_OACK => Self::OAck(TftpOAck::try_from(buf)?),
				consts::OPCODE_DATA => Self::Data(TftpData::try_from(buf)?),
				x => return Err(ParseError::InvalidOpcode(x)),
			}
		)
	}
}

// ############################################################################
// ############################################################################
// #### MUTABLE PACKETS #######################################################
// ############################################################################
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
	pub fn to_immutable(self) -> PacketBuf<'a> {
		match self {
			MutablePacketBuf::Borrowed(b) => PacketBuf::Borrowed(b),
			MutablePacketBuf::Owned(v) => PacketBuf::Owned(v)
		}
	}
}
impl AsRef<[u8]> for MutablePacketBuf<'_> {
	fn as_ref(&self) -> &[u8] {
		match self {
			MutablePacketBuf::Borrowed(b) => *b,
			MutablePacketBuf::Owned(v) => &v[..]
		}
	}
}
impl AsMut<[u8]> for MutablePacketBuf<'_> {
	fn as_mut(&mut self) -> &mut [u8] {
		match self {
			MutablePacketBuf::Borrowed(b) => *b,
			MutablePacketBuf::Owned(v) => &mut v[..]
		}
	}
}

/* pub struct MutableTftpReq<'a> {
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
} */

pub struct MutableTftpData<'a> { 
	buf: MutablePacketBuf<'a>,
	len: usize,
}
impl<'a> MutableTftpData<'a> {
	fn inner(&self) -> &[u8] {
		match self.buf {
			MutablePacketBuf::Borrowed(ref b) => *b,
			MutablePacketBuf::Owned(ref v) => &v[..],
		}
	}
	fn inner_mut(&mut self) -> &mut [u8] {
		match self.buf {
			MutablePacketBuf::Borrowed(ref mut b) => *b,
			MutablePacketBuf::Owned(ref mut v) => &mut v[..],
		}
	}

	pub fn try_from(buf: &'a mut [u8], is_filled: bool) -> Result<Self> {
		if buf.len() < 4 {
			return Err(ParseError::UnexpectedEof);
		}

		buf[0..=1].copy_from_slice(&consts::OPCODE_DATA.to_be_bytes());

		let buf_len = buf.len();
		Ok(Self { 
			buf: MutablePacketBuf::Borrowed(buf),
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
		
		Self { buf: MutablePacketBuf::Borrowed(buf), len: 4 + data.len() }
	}

	pub fn set_blocknum(&mut self, blocknum: u16) {
		let buf = self.inner_mut();
		buf[2..=3].copy_from_slice(blocknum.to_be_bytes().as_ref())
	}

	/// 
	/// This will panic if the buffer is too small!
	/// 
	pub fn set_data(&mut self, data: &[u8]) {
		let buf = self.inner_mut();
		if buf.len() < (4 + data.len()) {
			panic!();
		}

		super::utils::copy(data, &mut buf[4..]);
		self.len = 4 + data.len();
	}

	pub fn blocknum(&self) -> u16 {
		let buf = self.inner();
		u16::from_be_bytes([ buf[2], buf[3] ])
	}
	pub fn len(&self) -> usize { self.len }
}
impl<'a> Packet for MutableTftpData<'a> {
	fn packet_kind(&self) -> PacketKind {
		PacketKind::Data
	}

	fn as_bytes(&self) -> &[u8] {
		&self.inner()[..self.len]
	}
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
impl Packet for MutableTftpAck {
	fn packet_kind(&self) -> PacketKind {
		PacketKind::Ack
	}

	fn as_bytes(&self) -> &[u8] {
		&self.buf[..]
	}
}

/* pub struct MutableTftpOAck { 
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
} */


pub struct MutableTftpError<'a> { 
	buf: &'a mut [u8],
	data_len: usize,
}
impl<'a> MutableTftpError<'a> {

	///
	/// This panics in case the supplied buffer is too small!
	/// 
	pub fn with(buf: &'a mut [u8], err_code: ErrorCode, err_msg: &str) -> Self {
		let mut len: usize = 4;
		if buf.len() < (5 + err_msg.len()) {
			panic!()
		}

		buf[0..=1].copy_from_slice(&consts::OPCODE_ERROR.to_be_bytes()[..]);
		buf[2..=3].copy_from_slice(&(err_code as u16).to_be_bytes()[..]);
		if err_msg.len() > 0 && err_msg.is_ascii() {
			let max_len = buf.len() - 1;
			let copied = utils::copy(err_msg.as_bytes(), &mut buf[4..max_len]);
			len += copied;
		}
		buf[len] = 0;

		Self { buf, data_len: len }
	}

	pub fn set_error_code(&mut self, code: ErrorCode) {
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
	//OAck(MutableTftpOAck),
	Err(MutableTftpError<'a>),
}
impl<'a> MutableTftpPacket<'a> {
	pub fn as_bytes(&self) -> &[u8] {
		match self {
			Self::Data(p) => p.as_bytes(),
			Self::Err(p) => p.as_bytes(),
			//Self::OAck(p) => p.as_bytes(),
			Self::Ack(p) => p.as_bytes(),
		}
	}
}