use super::{Mode, RequestKind, TftpReq, PacketBuf, TftpOAck};
use super::super::options::TftpOption;
use super::super::consts;


pub struct TftpReqBuilder<'a> {
	buf: Vec<u8>,
	kind: RequestKind,
	mode: Mode,

	filename: &'a str,
	options: Option<&'a [TftpOption]>,
}
impl<'a> TftpReqBuilder<'a> {
	pub fn new() -> Self {
		TftpReqBuilder {
			buf: Vec::with_capacity(64),
			kind: RequestKind::Rrq,
			mode: Mode::Octet,
			filename: "",
			options: None
		}
	}

	#[inline] pub fn kind(mut self, kind: RequestKind) -> Self {
		self.kind = kind;
		self
	}
	#[inline] pub fn mode(mut self, mode: Mode) -> Self {
		self.mode = mode;
		self
	}

	#[inline] pub fn filename(mut self, filename: &'a str) -> Self {
		self.filename = filename;
		self
	}
	#[inline] pub fn options(mut self, options: &'a [TftpOption]) -> Self {
		self.options = Some(options);
		self
	}

	pub fn build<'b>(mut self) -> TftpReq<'b> {
		self.buf.extend((self.kind as u16).to_be_bytes());
		self.buf.extend(self.filename.as_bytes());
		self.buf.push(0);
		self.buf.extend(self.mode.as_str().as_bytes());
		self.buf.push(0);

		if let Some(opts) = self.options {
			for opt in opts {
				let opt_tuple = opt.as_str_tuple();
				self.buf.extend(opt_tuple.0.as_bytes());
				self.buf.push(0);
				self.buf.extend(opt_tuple.1.as_bytes());
				self.buf.push(0);
			}
		}

		TftpReq { buf: PacketBuf::Owned(self.buf) }
	}
}

pub struct TftpOAckBuilder {
	options: Vec<TftpOption>,
}
impl TftpOAckBuilder {
	pub fn new() -> Self {
		Self {
			options: Vec::with_capacity(3),
		}
	}

	pub fn option(mut self, option: TftpOption) -> Self {
		self.options.push(option);
		self
	}

	pub fn options(mut self, options: &[TftpOption]) -> Self {
		self.options.extend(options);
		self
	}

	pub fn build(self) -> TftpOAck<'static> {
		let mut buf: Vec<u8> = Vec::with_capacity(64);

		buf.extend(consts::OPCODE_OACK.to_be_bytes());
		for opt in self.options {
			let tuple = opt.as_str_tuple();
			buf.extend(tuple.0.as_bytes());
			buf.push(0);
			buf.extend(tuple.0.as_bytes());
			buf.push(0);
		}

		TftpOAck { buf: PacketBuf::Owned(buf) }
	}
}