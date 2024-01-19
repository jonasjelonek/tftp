use std::io::Write;

use crate::tftp::{
	consts,
	utils,

	packet::TftpReq,
	packet::TftpOAck,
	packet::TftpError,

	options::TftpOption,

	Mode, RequestKind, ErrorCode,
};


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

	fn write_to_buf(&mut self, buf: &mut [u8]) -> usize {
		buf[0..=1].copy_from_slice((self.kind as u16).to_be_bytes().as_slice());

		let mut written: usize = 2;
		let mut buf_ref = &mut buf[2..];
		written += buf_ref.write(self.filename.as_bytes()).unwrap_or(0);
		written += buf_ref.write(&[ 0 ]).unwrap_or(0);
		written += buf_ref.write(self.mode.as_str().as_bytes()).unwrap_or(0);
		written += buf_ref.write(&[ 0 ]).unwrap_or(0);

		if let Some(opts) = self.options {
			for opt in opts {
				let tuple = opt.as_str_tuple();
				written += buf_ref.write(tuple.0.as_bytes()).unwrap_or(0);
				written += buf_ref.write(&[ 0 ]).unwrap_or(0);
				written += buf_ref.write(tuple.1.as_bytes()).unwrap_or(0);
				written += buf_ref.write(&[ 0 ]).unwrap_or(0);
			}
		}

		written
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
				let len = self.write_to_buf(&mut buf[..]);
				buf.truncate(len);

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

	/// Assigns a buffer to this builder. This way the builder can be used with
	/// a stack-allocated buffer instead of a heap-allocated Vec<>, which is
	/// used by default.
	/// 
	/// **Make sure that the buffer is big enough for the expected content!
	/// Building will silently fail when the buffer is too small, maybe resulting
	/// in a corrupted packet.**
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

	fn write_to_buf(&mut self, buf: &mut [u8]) -> usize {
		buf[0..=1].copy_from_slice(consts::OPCODE_OACK.to_be_bytes().as_slice());

		let mut written: usize = 2;
		let mut buf_opt = &mut buf[2..];
		for opt in self.options.iter() {
			let tuple = opt.as_str_tuple();
			written += buf_opt.write(tuple.0.as_bytes()).unwrap_or(0);
			written += buf_opt.write(&[ 0 ]).unwrap_or(0);
			written += buf_opt.write(tuple.1.as_bytes()).unwrap_or(0);
			written += buf_opt.write(&[ 0 ]).unwrap_or(0);
		}

		written
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
				let len = self.write_to_buf(&mut buf[..]);
				buf.truncate(len);

				TftpOAck::from_owned(buf)
			}
		}
	}
}

pub struct TftpErrorBuilder<'a> {
	buf: Option<&'a mut [u8]>,
	code: ErrorCode,
	msg: Option<&'a str>,
}
impl<'a> TftpErrorBuilder<'a> {
	pub fn new() -> Self {
		Self {
			buf: None, code: ErrorCode::NotDefined,
			msg: None
		}
	}

	/// Assigns a buffer to this builder. This way the builder can be used with
	/// a stack-allocated buffer instead of a heap-allocated Vec<>, which is
	/// used by default.
	/// 
	/// **Make sure that the buffer is big enough for the expected content!
	/// Building will silently fail when the buffer is too small, maybe resulting
	/// in a corrupted packet.**
	#[inline] pub fn with_buf(mut self, buf: &'a mut [u8]) -> Self {
		self.buf = Some(buf);
		self
	}
	#[inline] pub fn error_code(mut self, code: ErrorCode) -> Self {
		self.code = code;
		self
	}
	#[inline] pub fn error_msg(mut self, msg: &'a str) -> Self {
		self.msg = Some(msg);
		self
	}

	fn write_to_buf(&mut self, buf: &mut [u8]) -> usize {
		buf[0..=1].copy_from_slice(consts::OPCODE_ERROR.to_be_bytes().as_slice());
		buf[2..=3].copy_from_slice((self.code as u16).to_be_bytes().as_slice());
		
		let mut len: usize = 4;
		if let Some(msg) = self.msg {
			len += utils::copy(msg.as_bytes(), &mut buf[4..]);
		}
		
		match len == buf.len() {
			true => buf[len - 1] = 0,
			false => {
				buf[len] = 0;
				len += 1; /* account for null terminator */
			},
		}

		len
	}

	pub fn build(mut self) -> TftpError<'a> {
		let buf = self.buf.take();
		match buf {
			Some(buf) => {
				self.write_to_buf(buf);
				TftpError::from_borrowed(buf)
			},
			None => {
				let mut buf = vec![0; 64];
				let len = self.write_to_buf(&mut buf[..]);
				buf.truncate(len);

				TftpError::from_owned(buf)
			}
		}
	}
}