use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

use clap::{arg, command, ValueEnum};
use clap::{Parser, Subcommand};

use simple_logger::SimpleLogger;

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Options {
	#[arg(value_enum, short, long, 
		default_value_t = DebugLevel::Warn,
		help = "Debug level to determine which messages are printed"
	)]
	pub debug: DebugLevel,

	#[arg(short = 'r', long = "root")]
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

#[derive(Subcommand, Debug)]
pub enum RunMode {
	Server {
		#[arg(short, long, default_value_t = IpAddr::V4(Ipv4Addr::UNSPECIFIED))]
		bind: IpAddr,

		#[arg(short, long, default_value_t = crate::tftp::consts::TFTP_LISTEN_PORT)]
		port: u16,
	},
	Client {
		#[command(subcommand)]
		action: ClientAction
	}
}

#[derive(Subcommand, Debug)]
pub enum ClientAction {
	Get {
		file: PathBuf,
		server: IpAddr,

		#[arg(short, long, default_value_t = crate::tftp::consts::TFTP_LISTEN_PORT)]
		port: u16,

		//#[arg(short, long, value_enum, default_value_t = crate::tftp::Mode::Octet)]
		//mode: crate::tftp::Mode,

		#[arg(short, long)]
		blocksize: Option<u16>,

		#[arg(short, long)]
		timeout: Option<u8>
	},
	Put {
		file: PathBuf,
		server: IpAddr,

		#[arg(short, long, default_value_t = crate::tftp::consts::TFTP_LISTEN_PORT)]
		port: u16,

		//#[arg(short, long, value_enum, default_value_t = crate::tftp::Mode::Octet)]
		//mode: crate::tftp::Mode,

		#[arg(short, long)]
		blocksize: Option<u16>,

		#[arg(short, long)]
		timeout: Option<u8>
	}
}

pub fn init_logger(debug_level: DebugLevel) {
	SimpleLogger::new()
		.with_level(debug_level.into())
		.env()
		.init()
		.unwrap();
}