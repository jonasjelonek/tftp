use std::net::SocketAddr;

use crate::cli::ClientAction;

pub struct TftpClient {
	local_addr: SocketAddr,
	remote_addr: SocketAddr,

}
impl TftpClient {
	pub fn new() {

	}

	pub fn get(&mut self) {

	}

	pub fn put(&mut self) {

	}
}

pub async fn run_client(action: ClientAction) -> Result<(), String> {

	Ok(())
}