use std::fmt::Display;

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
impl Display for PacketError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::InvalidCharacters => write!(f, "Invalid characters"),
			Self::InvalidOpcode => write!(f, "Invalid opcode"),
			Self::UnexpectedOpcode => write!(f, "Unexpected opcode"),
			Self::MalformedPacket => write!(f, "Malformed packet"),
			Self::NotNullTerminated => write!(f, "Missing null termination"),
			Self::UnexpectedEof => write!(f, "Unexpected EOF"),
			Self::UnknownTxMode => write!(f, "Unknown transfer mode"),
		}
	}
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
	type Error = PacketError;

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
			_ => Err(PacketError::MalformedPacket)
		}
	}
}

pub struct ParseModeError;
impl Display for ParseModeError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "")
	}
}