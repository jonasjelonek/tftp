#![feature(let_chains)]

pub mod cli;
pub mod tftp;
pub mod server;
pub mod client;

use std::path::PathBuf;
use std::sync::OnceLock;

#[allow(unused)]
use log::{info, warn, error, debug, trace};
use tokio_util::sync::CancellationToken;
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

	cli::init_logger(options.debug);

	/* Init our root directory */
	if let Some(root_dir) = options.root_dir {
		let root = match PathBuf
			::from(&shellexpand::tilde(&root_dir.to_string_lossy())[..])
			.canonicalize()
		{
			Ok(p) => p,
			Err(e) => return error!("Invalid root dir path '{}': {}", root_dir.display(), e),
		};

		match root.try_exists() {
			Ok(true) => WORKING_DIR.set(root.clone()).unwrap_or(()),
			_ => return error!("Cannot find/access specified root path!")
		}
	} else {
		match std::env::current_dir() {
			Ok(wd) => WORKING_DIR.set(wd).unwrap(),
			Err(e) => return error!("Cannot access current working dir: {}!", e),
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

	match options.run_mode {
		cli::RunMode::Server { bind, port } => {
			server::run_server((bind, port).into(), cancel_token).await
		},
		cli::RunMode::Client { client_opts, action } => {
			client::run_client(action, client_opts, cancel_token).await.unwrap()
		},
	};

	/*if let Err(e) = res {
		return error!("{}", e);
	}*/
}
