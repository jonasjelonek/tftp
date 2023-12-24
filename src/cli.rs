use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

use clap::{arg, command, ValueEnum};
use clap::{Parser, Subcommand};
use clap_utils::EnumString;

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

#[derive(Debug, Clone, ValueEnum, Default, EnumString)]
pub enum DebugLevel {
	#[strum(serialize = "off")]
	Off = 0,

	#[strum(serialize = "err", serialize = "error")]
	Error,

	#[default]
	#[strum(serialize = "warn")]
	Warn,

	#[strum(serialize = "info")]
	Info,

	#[strum(serialize = "debug")]
	Debug,

	#[strum(serialize = "trace")]
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
	Client { }
}

pub fn init_logger(debug_level: DebugLevel) {
	SimpleLogger::new()
		.with_level(debug_level.into())
		.env()
		.init()
		.unwrap();
}