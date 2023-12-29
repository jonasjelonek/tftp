use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

use clap::{arg, command, ValueEnum, Args};
use clap::{Parser, Subcommand};

use simple_logger::SimpleLogger;

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

#[derive(Debug, Clone, ValueEnum, Default)]
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
	blocksize: u16,

	#[arg(short, long, default_value_t = crate::tftp::consts::DEFAULT_TIMEOUT_SECS)]
	timeout: u8,

	#[arg(
		short = 'T', long, default_value_t = false,
		help = "Request (for RRQ) or hand over (for WRQ) the size of the file"
	)]
	transfer_size: bool,

	//#[arg(short, long, value_enum, default_value_t = crate::tftp::Mode::Octet)]
	//mode: crate::tftp::Mode,
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
	file: PathBuf,

	#[arg(help = "The remote server to connect to.")]
	server: IpAddr,

	#[arg(
		default_value_t = crate::tftp::consts::TFTP_LISTEN_PORT,
		help = "(optional) The remote port to connect to."
	)]
	port: u16,
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

pub fn init_logger(debug_level: DebugLevel) {
	SimpleLogger::new()
		.with_level(debug_level.into())
		.env()
		.init()
		.unwrap();
}