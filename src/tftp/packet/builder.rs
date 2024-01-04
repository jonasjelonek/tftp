use std::io::Write;

use super::{Mode, RequestKind, TftpReq, PacketBuf, TftpOAck};
use super::super::options::TftpOption;
use super::super::consts;


pub struct TftpReqBuilder<'a, 'b> {
	buf: Option<&'a mut [u8]>,
	kind: RequestKind,
	mode: Mode,

	filename: &'b str,
	options: Option<&'b [TftpOption]>,
}
impl<'a, 'b> TftpReqBuilder<'a, 'b> {
	pub fn new() -> Self {
		TftpReqBuilder {
			buf: None,
			kind: RequestKind::Rrq,
			mode: Mode::Octet,
			filename: "",
			options: None
		}
	}

	/// Assigns a buffer to this builder. This way the builder can be used with
	/// a stack-allocated buffer instead of a heap-allocated Vec<>, which is
	/// used by default.
	/// 
	/// **Make sure that the buffer is big enough for the expected content!
	/// Building will silently fail when the buffer is too small, maybe resulting
	/// in a corrupted packet.**
	/// 
	#[inline] pub fn with_buf(mut self, buf: &'a mut [u8]) -> Self {
		self.buf = Some(buf);
		self
	}

	#[inline] pub fn kind(mut self, kind: RequestKind) -> Self {
		self.kind = kind;
		self
	}
	#[inline] pub fn mode(mut self, mode: Mode) -> Self {
		self.mode = mode;
		self
	}

	#[inline] pub fn filename(mut self, filename: &'b str) -> Self {
		self.filename = filename;
		self
	}
	#[inline] pub fn options(mut self, options: &'b [TftpOption]) -> Self {
		self.options = Some(options);
		self
	}

	fn write_to_buf(&mut self, buf: &mut [u8]) {
		buf[0..=1].copy_from_slice((self.kind as u16).to_be_bytes().as_slice());

		let mut buf_ref = &mut buf[2..];
		let _ = buf_ref.write(self.filename.as_bytes());
		let _ = buf_ref.write(&[ 0 ]);
		let _ = buf_ref.write(self.mode.as_str().as_bytes());
		let _ = buf_ref.write(&[ 0 ]);

		if let Some(opts) = self.options {
			for opt in opts {
				let tuple = opt.as_str_tuple();
				let _ = buf_ref.write(tuple.0.as_bytes());
				let _ = buf_ref.write(&[ 0 ]);
				let _ = buf_ref.write(tuple.1.as_bytes());
				let _ = buf_ref.write(&[ 0 ]);
			}
		}
	}

	pub fn build(mut self) -> TftpReq<'a> {
		let buf = self.buf.take();
		match buf {
			Some(buf) => {
				self.write_to_buf(buf);
				TftpReq::from_borrowed(buf)
			},
			None => {
				let mut buf = vec![0; 64];
				self.write_to_buf(&mut buf[..]);
				TftpReq::from_owned(buf)
			}
		}
	}
}

pub struct TftpOAckBuilder<'a> {
	buf: Option<&'a mut [u8]>,
	options: Vec<TftpOption>,
}
impl<'a> TftpOAckBuilder<'a> {
	pub fn new() -> Self {
		Self {
			buf: None,
			options: Vec::with_capacity(3),
		}
	}

	#[inline] pub fn with_buf(mut self, buf: &'a mut [u8]) -> Self {
		self.buf = Some(buf);
		self
	}
	#[inline] pub fn option(mut self, option: TftpOption) -> Self {
		self.options.push(option);
		self
	}
	#[inline] pub fn options(mut self, options: &[TftpOption]) -> Self {
		self.options.extend(options);
		self
	}

	fn write_to_buf(&mut self, buf: &mut [u8]) {
		buf[0..=1].copy_from_slice(consts::OPCODE_OACK.to_be_bytes().as_slice());
		let mut buf_opt = &mut buf[2..];
		for opt in self.options.iter() {
			let tuple = opt.as_str_tuple();
			let _ = buf_opt.write(tuple.0.as_bytes());
			let _ = buf_opt.write(&[ 0 ]);
			let _ = buf_opt.write(tuple.1.as_bytes());
			let _ = buf_opt.write(&[ 0 ]);
		}
	}

	pub fn build(mut self) -> TftpOAck<'a> {
		let buf = self.buf.take();
		match buf {
			Some(buf) => {
				self.write_to_buf(buf);
				TftpOAck::from_borrowed(buf)
			},
			None => {
				let mut buf = vec![0; 64];
				self.write_to_buf(&mut buf[..]);
				TftpOAck::from_owned(buf)
			}
		}
	}
}