use std::fmt::Display;
use thiserror::Error;

use crate::tftp::consts;

#[derive(Debug, Error)]
pub enum RequestError {
	#[error("received response from an unknown peer")]
	UnknownPeer,
	#[error("the requested file could not be found")]
	FileNotFound,
	#[error("the file is not accessible for reading/writing")]
	FileNotAccessible,
	#[error("option negotiation with peer failed: {0}")]
	OptionNegotiationFailed(#[from] OptionError),
	#[error("")]
	MalformedRequest,
	#[error("")]
	ConnectionError(#[from] ConnectionError),
	#[error("")]
	OtherHostError(std::io::Error)
}

#[derive(Debug, Error)]
pub enum ConnectionError {
	#[error("connection/transfer was cancelled by the host")]
	Cancelled,
	#[error("received an unexpected packet")]
	UnexpectedPacket,
	#[error("received ACK for an unexpected block")]
	UnexpectedBlockAck,
	#[error("timeout occured")]
	Timeout,
	#[error("received response with an unknown TID")]
	UnknownTid,
	#[error("peer requested an unsupported transfer mode")]
	UnsupportedTxMode,
	#[error("")]
	PeerError(#[from] TftpError),
	#[error("response is invalid: {0}")]
	InvalidResponse(#[from] ParseError),
	#[error("input/output error: {0}")]
	IO(#[from] std::io::Error)
}

#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum ParseError {
	#[error("unexpected EOF")]
	UnexpectedEof,
	#[error("malformed packet")]
	MalformedPacket,
	#[error("unexpected opcode while parsing packet")]
	UnexpectedOpcode,
	#[error("{0} is not valid opcode")]
	InvalidOpcode(u16),
	#[error("null terminator is missing")]
	NotNullTerminated,
	#[error("string contains non-ascii characters")]
	NotAscii,
	#[error("unknown transfer mode")]
	UnknownTxMode,
}

impl From<std::ffi::FromBytesUntilNulError> for ParseError {
	fn from(_: std::ffi::FromBytesUntilNulError) -> Self {
		Self::NotNullTerminated
	}
}
impl From<std::str::Utf8Error> for ParseError {
	fn from(_: std::str::Utf8Error) -> Self {
		Self::NotAscii
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum OptionError {
	#[error("the option is invalid")]
	InvalidOption,
	#[error("client didn't acknowledge options")]
	NoAck,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u16)]
pub enum ErrorCode {
	NotDefined = consts::ERR_NOTDEFINED,
	FileNotFound = consts::ERR_FILENOTFOUND,
	AccessViolation = consts::ERR_ACCESSVIOLATION,
	StorageError = consts::ERR_STORAGEERROR,
	IllegalOperation = consts::ERR_ILLEGALOPERATION,
	UnknownTid = consts::ERR_UNKNOWNTID,
	FileExists = consts::ERR_FILEEXISTS,
	NoSuchUser = consts::ERR_NOSUCHUSER,
	InvalidOption = consts::ERR_INVALIDOPTION,
}
impl Display for ErrorCode {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", *self as u16)
	}
}
impl TryFrom<u16> for ErrorCode {
	type Error = ParseError;

	fn try_from(value: u16) -> Result<Self, Self::Error> {
		match value {
			consts::ERR_NOTDEFINED => Ok(Self::NotDefined),
			consts::ERR_FILENOTFOUND => Ok(Self::FileNotFound),
			consts::ERR_ACCESSVIOLATION => Ok(Self::AccessViolation),
			consts::ERR_STORAGEERROR => Ok(Self::StorageError),
			consts::ERR_ILLEGALOPERATION => Ok(Self::IllegalOperation),
			consts::ERR_UNKNOWNTID => Ok(Self::UnknownTid),
			consts::ERR_FILEEXISTS => Ok(Self::FileExists),
			consts::ERR_NOSUCHUSER => Ok(Self::NoSuchUser),
			consts::ERR_INVALIDOPTION => Ok(Self::InvalidOption),
			_ => Err(ParseError::MalformedPacket)
		}
	}
}

#[derive(Debug, Error)]
pub struct TftpError {
	code: ErrorCode,
	msg: Box<str>
}
impl Display for TftpError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{};{}", self.code, self.msg)
	}
}
impl<'a> From<crate::tftp::packet::TftpError<'a>> for TftpError {
	fn from(value: crate::tftp::packet::TftpError) -> Self {
		TftpError { 
			code: value.error_code(),
			msg: value.error_msg().into()
		}
	}
}