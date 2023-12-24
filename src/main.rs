#![feature(iter_advance_by)]
#![feature(let_chains)]
#![feature(once_cell_try)]

pub mod cli;
pub mod tftp;
pub mod server;
pub mod client;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::OnceLock;
use tokio_util::sync::CancellationToken;

use log::{info, warn, error, debug, trace};
use clap::Parser;


static WORKING_DIR: OnceLock<PathBuf> = OnceLock::new();

/**
 * Shortcut used to get working dir without needing to check or unwrap everytime.
 * This OnceLock will be initialized in early main, if init fails then we stop the program.
 * Thus, it's safe to just unwrap it.
 */
fn working_dir<'a>() -> &'a PathBuf {
	WORKING_DIR.get().unwrap()
}

#[tokio::main]
async fn main() {
	let options = cli::Options::parse();

	/* Initialize logging facility; can unwrap here because it has a default value */
	cli::init_logger(options.debug);

	/* Handle the global args here */
	if let Some(root_dir) = options.root_dir {
		match root_dir.try_exists() {
			Ok(true) => WORKING_DIR.set(root_dir.clone()).unwrap_or(()),
			_ => return error!("Cannot find/access specified root path!")
		}
	} else {
		if let Err(e) = WORKING_DIR.get_or_try_init(std::env::current_dir) {
			return error!("Cannot access current working dir: {}!", e);
		}
	}

	/* From here on its safe to read + unwrap all globals, they are either initialised or we weren't here */
	debug!("working dir '{}'", working_dir().display());

	let cancel_token: CancellationToken = CancellationToken::new();
	let sigint_token = cancel_token.clone();

	/* Let's handle SIGINT on our own to gracefully shutdown all tasks */
	ctrlc::set_handler(move || {
		info!("Received SIGINT");
		sigint_token.cancel();
	}).unwrap();

	let res = match options.run_mode {
		cli::RunMode::Server { bind, port } => {
			let listen_addr = SocketAddr::from((bind, port));
			server::server_task(listen_addr, cancel_token).await
		},
		cli::RunMode::Client { } => todo!()
	};

	if let Err(e) = res {
		return error!("{}", e);
	}

	// Moving the above part after setting the SIGINT handler into a task and then awaiting it breaks the logger somehow!
	// Messages are extremely delayed, probably due to blocking the main task somehow.
}