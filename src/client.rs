use std::fs::OpenOptions;
use std::path::PathBuf;
use std::net::{SocketAddr, IpAddr, Ipv4Addr};
use std::io;

use tokio_util::sync::CancellationToken;

#[allow(unused)]
use log::{info, warn, error, debug, trace};

use crate::cli;
use crate::tftp::options::{TftpOption, TftpOptionKind};
use crate::tftp::{self, Mode, RequestKind, TftpConnection};
use crate::tftp::packet::{builder::*, TftpPacket};
use crate::tftp::error::{ConnectionError, RequestError};

pub type Result<T> = std::result::Result<T, RequestError>;

pub struct TftpRequestParameters<'a> {
	pub req_kind: RequestKind,
	pub server: SocketAddr,
	pub file: PathBuf,
	pub options: &'a [TftpOption],
}

pub struct TftpClient {
	local_addr: IpAddr,
	cxl_token: CancellationToken,
	options: Vec<TftpOption>,
}
impl TftpClient {
	pub fn new(local_addr: IpAddr, cxl_token: CancellationToken) -> Self {
		Self {
			local_addr,
			cxl_token,
			options: Vec::new()
		}
	}

	pub fn add_option(&mut self, option: &TftpOption) {
		for x in 0..self.options.len() {
			if self.options[x].kind() == option.kind() {
				self.options[x] = option.clone();
				return;
			}
		}

		self.options.push(option.clone())
	}

	pub async fn get(&mut self, path: PathBuf, server: SocketAddr) -> Result<()> {
		let mut conn = TftpConnection::new(self.local_addr, self.cxl_token.clone())?;

		let filename = path.file_name().ok_or(RequestError::FileNotFound)?.to_string_lossy();
		let file = match OpenOptions::new().create(true).write(true).truncate(true).open(&path) {
			Ok(f) => f,
			Err(e) if e.kind() == io::ErrorKind::PermissionDenied => return Err(RequestError::FileNotAccessible),
			Err(e) => return Err(RequestError::OtherHostError(e))
		};
		
		let mut builder = TftpReqBuilder::new()
			.kind(RequestKind::Rrq)
			.mode(Mode::Octet)
			.filename(&filename);

		if self.options.len() > 0 {
			builder = builder.options(&self.options[..]);
		}
		let pkt = builder.build();
		conn.send_request_to(&pkt, server)?;

		/* Handle the first packet coming from the server here instead of in receive_file.
		 * We don't know which port the server will use to reply, and handling this should
		 * not be done in TftpConnection's receive functions.
		 * In case we requested options, we need to handle the first packet anyway. */
		let mut buf = [0u8; 4 + tftp::consts::DEFAULT_BLOCK_SIZE as usize];
		let (pkt, remote) = conn.receive_packet_from(&mut buf)?;

		// Fail if another IP is used
		if remote.ip() != server.ip() {
			return Err(RequestError::UnknownPeer);
		}
		conn.connect_to(remote)?;

		let mut init_data: Option<_> = None;
		match pkt {
			TftpPacket::OAck(oack) => {
				let opts = tftp::options::parse_tftp_options(
					oack.options().map_err(|e| ConnectionError::from(e))?
				)?;
				conn.set_options(&opts[..]);

				let ack_pkt = tftp::packet::MutableTftpAck::new(0);
				conn.send_packet(&ack_pkt)?;
			},
			TftpPacket::Data(data) => init_data = Some(data),
			_ => return Err(ConnectionError::UnexpectedPacket.into()),
		}
		conn.receive_data(file, init_data).await?;
		Ok(())
	}

	pub async fn put(&mut self, path: PathBuf, server: SocketAddr) -> Result<()> {
		let mut conn = TftpConnection::new(self.local_addr, self.cxl_token.clone())?;

		let filename = path.file_name().ok_or(RequestError::FileNotFound)?.to_string_lossy();
		let file = match OpenOptions::new().read(true).open(&path) {
			Ok(f) => f,
			Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(RequestError::FileNotFound),
			Err(e) if e.kind() == io::ErrorKind::PermissionDenied => return Err(RequestError::FileNotAccessible),
			Err(e) => return Err(RequestError::OtherHostError(e))
		};

		let mut builder = TftpReqBuilder::new()
			.kind(RequestKind::Wrq)
			.mode(Mode::Octet) // we only support octet mode
			.filename(&filename);

		let mut options = self.options.to_owned();
		if options.len() > 0 {
			if let Some(i) = options.iter().position(|e| e.kind() == TftpOptionKind::TransferSize) {
				options[i] = TftpOption::TransferSize(file.metadata()?.len() as u32);
			}
			builder = builder.options(&options[..]);
		}
		let pkt = builder.build();
		conn.send_request_to(&pkt, server)?;

		let mut buf = [0u8; 64];
		let (pkt, remote) = conn.receive_packet_from(&mut buf)?;
		
		if remote.ip() != server.ip() {
			return Err(RequestError::UnknownPeer);
		}
		conn.connect_to(remote).ok();

		match pkt {
			TftpPacket::OAck(oack) => {
				let opts = tftp::options::parse_tftp_options(
					oack.options().map_err(|e| ConnectionError::from(e))?
				)?;
				conn.set_options(&opts[..]);
			},
			TftpPacket::Ack(_) => (),
			_ => return Err(ConnectionError::UnexpectedPacket.into())
		}
		
		conn.send_data(file).await?;
		Ok(())
	}
}

pub async fn run_client(action: cli::ClientAction, opts: cli::ClientOpts, root: PathBuf, cxl_token: CancellationToken) -> Result<()> {
	let local_addr = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));
	let mut client = TftpClient::new(local_addr, cxl_token);

	let req_opts = action.get_opts();
	let mut file_path = root;
	file_path.push(&req_opts.file.to_string_lossy()[..]);

	cli::parse_tftp_options(opts)
		.iter()
		.for_each(|opt| client.add_option(opt));

	let server = (req_opts.server, req_opts.port).into();
	match action.as_req_kind() {
		RequestKind::Rrq => client.get(file_path, server).await,
		RequestKind::Wrq => client.put(file_path, server).await
	}
}