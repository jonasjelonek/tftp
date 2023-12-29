use std::time::Duration;
use std::collections::HashMap;

use super::consts as consts;
use super::packet as packet;

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

pub struct OptionNegotiation {
	options: Vec<TftpOption>
}
impl OptionNegotiation {
	pub fn new() -> Self {
		Self { options: Vec::with_capacity(4) }
	}

	pub fn find_option(&self, kind: TftpOptionKind) -> Option<&TftpOption> {
		for opt in self.options.iter() {
			if opt.kind() == kind {
				return Some(opt);
			}
		}
		None
	}

	pub fn find_option_mut(&mut self, kind: TftpOptionKind) -> Option<&mut TftpOption> {
		for opt in self.options.iter_mut() {
			if opt.kind() == kind {
				return Some(opt);
			}
		}
		None
	}

	pub fn add_option(&mut self, option: TftpOption) {
		self.options.push(option)
	}

	pub fn options(&self) -> &Vec<TftpOption> {
		&self.options
	}

	pub fn parse_options(raw_opts: HashMap<&str, &str>) -> Result<Self, ()> {
		let mut res = Self::new();

		if let Some(val) = raw_opts.get(consts::OPT_BLOCKSIZE_IDENT) {
			if let Ok(size) = u16::from_str_radix(*val, 10) {
				res.add_option(TftpOption::Blocksize(size));
			} else { return Err(()); }
		}

		if let Some(val) = raw_opts.get(consts::OPT_TIMEOUT_IDENT) {
			if let Ok(timeout) = u8::from_str_radix(*val, 10) {
				res.add_option(TftpOption::Timeout(Duration::from_secs(timeout as u64)));
			} else { return Err(()); }
		}

		if let Some(val) = raw_opts.get(consts::OPT_TRANSFERSIZE_IDENT) {
			if let Ok(tf_size) = u32::from_str_radix(*val, 10) {
				res.add_option(TftpOption::TransferSize(tf_size));
			} else { return Err(()); }
		}

		Ok(res)
	}

	pub fn build_oack_packet(&self) -> packet::MutableTftpOAck {
		let mut pkt = packet::MutableTftpOAck::with_capacity(64);

		for opt in self.options.iter() {
			let (key, val) = match opt {
				TftpOption::Blocksize(ref sz) => (consts::OPT_BLOCKSIZE_IDENT, sz.to_string()),
				TftpOption::Timeout(ref to) => (consts::OPT_TIMEOUT_IDENT, to.as_secs().to_string()),
				TftpOption::TransferSize(ref tsz) => (consts::OPT_TRANSFERSIZE_IDENT, tsz.to_string()),
			};

			pkt.add_option(key, val.as_str());
		}

		pkt
	}
}

pub struct TftpOptions {
	pub blocksize: u16,
	pub timeout: Duration,
	pub transfer_size: u32
}
impl TftpOptions {
	pub fn merge_from(&mut self, neg_opts: &OptionNegotiation) {
		for opt in neg_opts.options() {
			match opt {
				TftpOption::Blocksize(sz) => self.blocksize = *sz,
				TftpOption::Timeout(to) => self.timeout = *to,
				TftpOption::TransferSize(tsz) => self.transfer_size = *tsz,
			}
		}
	}
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