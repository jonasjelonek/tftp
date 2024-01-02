use std::time::Duration;
use std::collections::HashMap;

use super::consts as consts;

#[derive(Debug)]
pub enum OptionError {
	InvalidOption,
	UnsupportedOption,
	UnexpectedValue,
	NoAck,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum TftpOptionKind {
	Blocksize,
	Timeout,
	TransferSize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TftpOption {
	Blocksize(u16),
	Timeout(Duration),
	TransferSize(u32),
}
impl TftpOption {
	pub fn kind(&self) -> TftpOptionKind {
		match self {
			Self::Blocksize(_) => TftpOptionKind::Blocksize,
			Self::Timeout(_) => TftpOptionKind::Timeout,
			Self::TransferSize(_) => TftpOptionKind::TransferSize,
		}
	}
	pub fn as_str_tuple(&self) -> (&'static str, String) {
		match self {
			Self::Blocksize(bs) => (consts::OPT_BLOCKSIZE_IDENT, bs.to_string()),
			Self::Timeout(t) => (consts::OPT_TIMEOUT_IDENT, t.as_secs().to_string()),
			Self::TransferSize(ts) => (consts::OPT_TRANSFERSIZE_IDENT, ts.to_string()),
		}
	}
}

pub fn parse_tftp_options(raw_opts: HashMap<&str, &str>) -> Result<Vec<TftpOption>, ()> {
	let mut res: Vec<TftpOption> = Vec::with_capacity(3);

	if let Some(val) = raw_opts.get(consts::OPT_BLOCKSIZE_IDENT) {
		if let Ok(size) = u16::from_str_radix(*val, 10) {
			res.push(TftpOption::Blocksize(size));
		} else { return Err(()); }
	}

	if let Some(val) = raw_opts.get(consts::OPT_TIMEOUT_IDENT) {
		if let Ok(timeout) = u8::from_str_radix(*val, 10) {
			res.push(TftpOption::Timeout(Duration::from_secs(timeout as u64)));
		} else { return Err(()); }
	}

	if let Some(val) = raw_opts.get(consts::OPT_TRANSFERSIZE_IDENT) {
		if let Ok(tf_size) = u32::from_str_radix(*val, 10) {
			res.push(TftpOption::TransferSize(tf_size));
		} else { return Err(()); }
	}

	Ok(res)
}

pub struct TftpOptions {
	pub blocksize: u16,
	pub timeout: Duration,
	pub transfer_size: u32
}
impl Default for TftpOptions {
	fn default() -> Self {
		Self { 
			blocksize: consts::DEFAULT_BLOCK_SIZE, 
			timeout: Duration::from_secs(consts::DEFAULT_TIMEOUT_SECS as u64), 
			transfer_size: 0,
		}
	}
}