use super::{Mode, RequestKind, TftpReq, PacketBuf};
use super::super::options::TftpOption;


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

	pub fn build(mut self) -> TftpReq<'a> {
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