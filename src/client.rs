use std::fs::OpenOptions;
use std::path::PathBuf;
use std::net::{SocketAddr, IpAddr, Ipv4Addr};

use tokio_util::sync::CancellationToken;

#[allow(unused)]
use log::{info, warn, error, debug, trace};

use crate::cli;
use crate::tftp::options::{TftpOption, TftpOptionKind};
use crate::tftp::{self, RequestKind, Mode, TftpConnection};
use crate::tftp::packet::{PacketKind, TftpPacket};
use crate::tftp::packet::builder::*;
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
}
impl TftpClient {
	pub fn new(local_addr: IpAddr, cxl_token: CancellationToken) -> Self {
		Self {
			local_addr,
			cxl_token
		}
	}

	pub async fn request<'a>(&mut self, req_params: TftpRequestParameters<'a>) {
		let conn = match TftpConnection::new(
			self.local_addr, self.cxl_token.clone()
		) {
			Ok(con) => con,
			Err(e) => return error!("failed to do request due to lower layer error: {}", e),
		};

		let result = match req_params.req_kind {
			RequestKind::Rrq => self.get(conn, req_params.server, req_params.file, req_params.options).await,
			RequestKind::Wrq => self.put(conn, req_params.server, req_params.file, req_params.options).await,
		};

	}

	async fn get(&mut self, mut conn: TftpConnection, server: SocketAddr, file_path: PathBuf, options: &[TftpOption]) -> Result<()> {
		let filename = file_path.file_name().unwrap().to_string_lossy();
		let file = match OpenOptions::new().create(true).write(true).truncate(true).open(&file_path) {
			Ok(f) => f,
			Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return Err(RequestError::FileNotAccessible),
			Err(e) => return Err(RequestError::OtherHostError(e))
		};
		
		let mut builder = TftpReqBuilder::new()
			.kind(RequestKind::Rrq)
			.mode(Mode::Octet)
			.filename(&filename);

		if options.len() > 0 {
			builder = builder.options(options);
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
				let opts = tftp::options::parse_tftp_options(oack.options().unwrap()).unwrap();
				conn.set_options(&opts[..]);

				let ack_pkt = tftp::packet::MutableTftpAck::new(0);
				conn.send_packet(&ack_pkt)?;
			},
			TftpPacket::Data(data) => init_data = Some(data),
			_ => return Err(ConnectionError::UnexpectedPacket.into()),
		}
		tftp::receive_data(conn, file, init_data).await?;
		Ok(())
	}

	async fn put(&mut self, mut conn: TftpConnection, server: SocketAddr, file_path: PathBuf, options: &[TftpOption]) -> Result<()> {
		let filename = file_path.file_name().unwrap().to_string_lossy();
		let file = match OpenOptions::new().read(true).open(&file_path) {
			Ok(f) => f,
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(RequestError::FileNotFound),
			Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return Err(RequestError::FileNotAccessible),
			Err(e) => return Err(RequestError::OtherHostError(e))
		};

		let mut builder = TftpReqBuilder::new()
			.kind(RequestKind::Wrq)
			.mode(Mode::Octet) // we only support octet mode
			.filename(&filename);

		let mut options = options.to_owned();
		if options.len() > 0 {
			if let Some(i) = options.iter().position(|e| e.kind() == TftpOptionKind::TransferSize) {
				options[i] = TftpOption::TransferSize(file.metadata().unwrap().len() as u32);
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
		conn.connect_to(remote).unwrap();

		match pkt {
			TftpPacket::OAck(oack) => {
				let opts = tftp::options::parse_tftp_options(oack.options().unwrap()).unwrap();
				conn.set_options(&opts[..]);
			},
			TftpPacket::Ack(_) => (),
			_ => return Err(ConnectionError::UnexpectedPacket.into())
		}
		
		tftp::send_data(conn, file).await?;
		Ok(())
	}
}

pub async fn run_client(action: cli::ClientAction, opts: cli::ClientOpts, cxl_token: CancellationToken) -> Result<()> {
	let local_addr = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));
	let mut client = TftpClient::new(local_addr, cxl_token);

	let req_opts = action.get_opts();
	let mut file_path = crate::working_dir().clone();
	file_path.push(&req_opts.file.to_string_lossy()[..]);

	let tftp_options = cli::parse_tftp_options(opts);
	let req_params = TftpRequestParameters {
		req_kind: action.as_req_kind(),
		server: (req_opts.server, req_opts.port).into(),
		file: file_path,
		options: &tftp_options[..]
	};

	client.request(req_params).await;
	Ok(())
}