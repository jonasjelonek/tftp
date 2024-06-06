use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::time::Duration;

use clap::{arg, command, ValueEnum, Args};
use clap::{Parser, Subcommand};

use simple_logger::SimpleLogger;

use crate::tftp;
use crate::tftp::options::TftpOption;

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Options {
	#[arg(value_enum, short, long, 
		default_value_t = DebugLevel::Warn,
		help = "Debug level to determine which messages are printed", global = true
	)]
	pub debug: DebugLevel,

	#[arg(short = 'r', long = "root", global = true)]
	pub root_dir: Option<PathBuf>,

	#[command(subcommand)]
	pub run_mode: RunMode,
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum DebugLevel {
	Off = 0,
	Error,

	#[default]
	Warn,
	Info,
	Debug,
	Trace
}
impl From<DebugLevel> for log::LevelFilter {
	fn from(value: DebugLevel) -> Self {
		match value {
			DebugLevel::Off => Self::Off,
			DebugLevel::Error => Self::Error,
			DebugLevel::Warn => Self::Warn,
			DebugLevel::Info => Self::Info,
			DebugLevel::Debug => Self::Debug,
			DebugLevel::Trace => Self::Trace,
		}
	}
}

#[derive(Debug, Args)]
pub struct ClientOpts {
	#[arg(short, long, default_value_t = crate::tftp::consts::DEFAULT_BLOCK_SIZE)]
	pub blocksize: u16,

	#[arg(
		short, long, default_value_t = crate::tftp::consts::DEFAULT_TIMEOUT_SECS,
		help = "Timout waiting for packet (in seconds)."
	)]
	pub timeout: u8,

	#[arg(
		short = 'T', long, default_value_t = false,
		help = "Request (for RRQ) or hand over (for WRQ) the size of the file."
	)]
	pub transfer_size: bool,
}

#[derive(Subcommand, Debug)]
pub enum RunMode {
	Server {
		#[arg(short, long, default_value_t = IpAddr::V4(Ipv4Addr::UNSPECIFIED))]
		bind: IpAddr,

		#[arg(short, long, default_value_t = crate::tftp::consts::TFTP_LISTEN_PORT)]
		port: u16,
	},
	Client {
		#[command(flatten)]
		client_opts: ClientOpts,

		#[command(subcommand)]
		action: ClientAction
	}
}

#[derive(Debug, Args)]
pub struct ClientActionOpts {
	pub file: PathBuf,

	#[arg(help = "The remote server to connect to.")]
	pub server: IpAddr,

	#[arg(
		default_value_t = crate::tftp::consts::TFTP_LISTEN_PORT,
		help = "(optional) The remote port to connect to."
	)]
	pub port: u16,
}

#[derive(Subcommand, Debug)]
pub enum ClientAction {
	Get {
		#[command(flatten)]
		opts: ClientActionOpts,
	},
	Put {
		#[command(flatten)]
		opts: ClientActionOpts,
	}
}
impl ClientAction {
	pub fn as_req_kind(&self) -> tftp::RequestKind {
		match self {
			Self::Get { opts: _ } => tftp::RequestKind::Rrq,
			Self::Put { opts: _ } => tftp::RequestKind::Wrq,
		}
	}

	pub fn get_opts(&self) -> &ClientActionOpts {
		match self {
			Self::Get { opts } => opts,
			Self::Put { opts } => opts,
		}
	}
}

pub fn parse_tftp_options(cli_opts: ClientOpts) -> Vec<TftpOption> {
	let mut v: Vec<TftpOption> = vec![];

	/* keep first to find it easier */
	if cli_opts.transfer_size {
		v.push(TftpOption::TransferSize(0));
	}
	if cli_opts.blocksize != tftp::consts::DEFAULT_BLOCK_SIZE {
		v.push(TftpOption::Blocksize(cli_opts.blocksize));
	}
	if cli_opts.timeout != tftp::consts::DEFAULT_TIMEOUT_SECS {
		v.push(TftpOption::Timeout(Duration::from_secs(cli_opts.timeout as u64)))
	}

	v
}

pub fn init_logger(debug_level: DebugLevel) {
	SimpleLogger::new()
		.with_level(debug_level.into())
		.env()
		.init()
		.unwrap();
}