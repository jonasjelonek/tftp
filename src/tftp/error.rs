use std::fmt::Display;
use thiserror::Error;

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
	#[error("response is invalid: {0}")]
	InvalidResponse(#[from] ParseError),
	#[error("input/output error: {0}")]
	IO(#[from] std::io::Error)
}

#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum ParseError {
	#[error("")]
	UnexpectedEof,
	#[error("")]
	MalformedPacket,
	#[error("")]
	UnexpectedOpcode,
	#[error("{0} is not valid opcode")]
	InvalidOpcode(u16),
	#[error("null terminator is missing")]
	NotNullTerminated,
	#[error("string contains non-ascii characters")]
	NotAscii,
	#[error("")]
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
	#[error("")]
	InvalidOption,
	#[error("")]
	UnsupportedOption,
	#[error("")]
	UnexpectedValue,
	#[error("")]
	NoAck,
}

#[derive(Debug, Clone, Copy, PartialEq)]
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
impl TryFrom<u16> for ErrorCode {
	type Error = ParseError;

	fn try_from(value: u16) -> Result<Self, Self::Error> {
		match value {
			0 => Ok(Self::NotDefined),
			1 => Ok(Self::FileNotFound),
			2 => Ok(Self::AccessViolation),
			3 => Ok(Self::StorageError),
			4 => Ok(Self::IllegalOperation),
			5 => Ok(Self::UnknownTid),
			6 => Ok(Self::FileExists),
			7 => Ok(Self::NoSuchUser),
			8 => Ok(Self::InvalidOption),
			_ => Err(ParseError::MalformedPacket)
		}
	}
}