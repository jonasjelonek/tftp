#![feature(let_chains)]

pub mod cli;
pub mod tftp;
pub mod server;
pub mod client;

use std::{error::Error, io, path::PathBuf};

#[allow(unused)]
use log::{info, warn, error, debug, trace};
use server::TftpServer;
use tokio_util::sync::CancellationToken;
use clap::Parser;

async fn run(opts: cli::Options) -> Result<(), Box<dyn Error>> {
	/* Init our root directory */
	let root_dir = match opts.root_dir {
		Some(rd) => {
			let root = PathBuf
				::from(shellexpand::tilde(&rd.to_string_lossy()).as_ref())
				.canonicalize()?;
			if root.try_exists()? {
				root
			} else {
				return Err(io::Error::from(io::ErrorKind::NotFound).into());
			}
		},
		_ => std::env::current_dir()?,
	};

	debug!("working dir '{}'", root_dir.display());

	let cancel_token: CancellationToken = CancellationToken::new();
	let sigint_token = cancel_token.clone();

	/* Let's handle SIGINT on our own to gracefully shutdown all tasks */
	ctrlc::set_handler(move || {
		info!("Received SIGINT");
		sigint_token.cancel();
	}).unwrap();

	match opts.run_mode {
		cli::RunMode::Server { bind, port } => {
			TftpServer::new((bind, port).into(), root_dir)?
				.run(cancel_token)
				.await?
		},
		cli::RunMode::Client { client_opts, action } => {
			client::run_client(action, client_opts, root_dir, cancel_token).await?
		},
	};

	Ok(())
}

#[tokio::main]
async fn main() {
	let options = cli::Options::parse();

	cli::init_logger(options.debug);

	match run(options).await {
		Ok(_) => (),
		Err(e) => error!("Error: {e}"),
	}
}
