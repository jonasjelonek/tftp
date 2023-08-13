#![feature(iter_advance_by)]
#![feature(let_chains)]
#![feature(once_cell_try)]

pub mod tftp;
pub mod server;
pub mod client;

use std::{path::PathBuf, net::IpAddr};
use std::sync::OnceLock;
use std::str::FromStr;
use tokio_util::sync::CancellationToken;

use log::{info, warn, error, debug, trace};
use simple_logger::{SimpleLogger, init};
use clap::{arg, command, value_parser, Command, ArgMatches};

static WORKING_DIR: OnceLock<PathBuf> = OnceLock::new();

/**
 * Shortcut used to get working dir without needing to check or unwrap everytime.
 * This OnceLock will be initialized in early main, if init fails then we stop the program.
 * Thus, it's safe to just unwrap it.
 */
fn working_dir<'a>() -> &'a PathBuf {
	WORKING_DIR.get().unwrap()
}

pub fn handle_cmd_args() -> ArgMatches {
	command!()
		.subcommand(
			Command::new("server")
			.about("act as a tftp server")
			.arg(arg!(-b --bind <ADDRESS> "Bind to a specific IP address (default 0.0.0.0)"))
			//.arg(arg!(-B --"bind-dev" <IFACE> "Bind to a specific interface instead of an IP"))
			.arg(
				arg!(-p --port <PORT> "The port to listen on for requests (default 69).")
				.value_parser(value_parser!(u16))
			)
		)
		.subcommand(
			Command::new("client")
			.about("act as a tftp client")
		)
		.arg(
			arg!(-d --"debug-level" <LEVEL> "One of the debug levels 'off', 'error', 'warn', 'info', 'debug' or 'trace'.")
			.default_value("warn")
			.global(true)
		)
		.arg(
			arg!(-r --root <PATH> "The root path / working dir for the TFTP server")
			.value_parser(value_parser!(PathBuf))
			.global(true)
		)
		.get_matches()
}

pub fn init_logger(debug_level: &str) {
	SimpleLogger::new()
		.with_level(log::LevelFilter::from_str(debug_level).unwrap())
		.env()
		.init()
		.unwrap();
}

pub fn init_globals(cmd_args: &ArgMatches) {
	/* Init working dir, either use specified one or current dir */
	if let Some(root) = cmd_args.get_one::<PathBuf>("root") {
		match root.try_exists() {
			Ok(true) => WORKING_DIR.set(root.clone()).unwrap_or(()),
			_ => return error!("Cannot find/access specified root path!")
		}
	} else {
		if let Err(e) = WORKING_DIR.get_or_try_init(std::env::current_dir) {
			return error!("Cannot access current working dir: {}!", e);
		}
	}
}

#[tokio::main]
async fn main() {
	let arg_matches = handle_cmd_args();

	/* Initialize logging facility; can unwrap here because it has a default value */
	init_logger(arg_matches.get_one::<String>("debug-level").unwrap());

	/* Handle the global args here */
	init_globals(&arg_matches);

	/* From here on its safe to read + unwrap all globals, they are either initialised or we weren't here */
	
	debug!("working dir '{}'", working_dir().display());

	let cancel_token: CancellationToken = CancellationToken::new();
	let main_task_token = cancel_token.clone();

	/* Let's handle SIGINT on our own to gracefully shutdown all tasks */
	ctrlc::set_handler(move || {
		info!("Received SIGINT");
		cancel_token.cancel();
	}).unwrap();

	let res = if let Some(subcmd_matches) = arg_matches.subcommand_matches("server") {
		let params = match server::prepare_server(subcmd_matches).await {
			Ok(pms) => pms,
			Err(e) => return error!("{}", e)
		};

		server::server_task(params, main_task_token).await
	} else if let Some(subcmd_matches) = arg_matches.subcommand_matches("client") {
		todo!()
	} else {
		Err(format!("Invalid or missing subcommand"))
	};

	if let Err(e) = res {
		return error!("{}", e);
	}

	// Moving the above part after setting the SIGINT handler into a task and then awaiting it breaks the logger somehow!
	// Messages are extremely delayed, probably due to blocking the main task somehow.
}