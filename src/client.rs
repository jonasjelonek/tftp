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

		match req_params.req_kind {
			RequestKind::Rrq => self.get(conn, req_params.server, req_params.file, req_params.options).await,
			RequestKind::Wrq => self.put(conn, req_params.server, req_params.file, req_params.options).await,
		}
	}

	async fn get(&mut self, mut conn: TftpConnection, server: SocketAddr, file_path: PathBuf, options: &[TftpOption]) {
		let filename = file_path.file_name().unwrap().to_string_lossy();
		let file = match OpenOptions::new().create(true).write(true).truncate(true).open(&file_path) {
			Ok(f) => f,
			Err(e) => return error!("Could not open file for GET request: {}", e),
		};

		let mut builder = TftpReqBuilder::new()
			.kind(RequestKind::Rrq)
			.mode(Mode::Octet)
			.filename(&filename);

		if options.len() > 0 {
			builder = builder.options(options);
		}
		let pkt = builder.build();
		conn.send_request_to(&pkt, server);

		/* Handle the first packet coming from the server here instead of in receive_file.
		 * We don't know which port the server will use to reply, and handling this should
		 * not be done in TftpConnection's receive functions.
		 * In case we requested options, we need to handle the first packet anyway. */
		let mut buf = [0u8; 4 + tftp::consts::DEFAULT_BLOCK_SIZE as usize];
		match conn.receive_packet_from(&mut buf, None) {
			Ok((pkt, remote)) if pkt.packet_kind() == PacketKind::OAck => {
				if remote.ip() != server.ip() {
					return conn.drop();
				}
				conn.connect_to(remote).unwrap();

				let TftpPacket::OAck(oack) = pkt else { unreachable!() };
				let opts = tftp::options::parse_tftp_options(oack.options().unwrap()).unwrap();
				conn.set_options(&opts[..]);

				let ack_pkt = tftp::packet::MutableTftpAck::new(0);
				let _ = conn.send_packet(&ack_pkt);

				tftp::receive_data(conn, file, None).await;
			},
			Ok((pkt, remote)) if pkt.packet_kind() == PacketKind::Data => {
				if remote.ip() != server.ip() {
					return conn.drop();
				}
				conn.connect_to(server).unwrap();

				let TftpPacket::Data(data) = pkt else { unreachable!() };
				tftp::receive_data(conn, file, Some(data)).await
			},
			Ok((pkt, _)) => error!("Received packet of unexpected kind {}", pkt.packet_kind()),
			Err(e) => error!("Server didn't properly respond to request ({})", e),
		};
	}

	async fn put(&mut self, mut conn: TftpConnection, server: SocketAddr, file_path: PathBuf, options: &[TftpOption]) {
		let filename = file_path.file_name().unwrap().to_string_lossy();
		let file = match OpenOptions::new().read(true).open(&file_path) {
			Ok(f) => f,
			Err(e) => return error!("Could not open file for PUT request: {}", e),
		};

		let mut builder = TftpReqBuilder::new()
			.kind(RequestKind::Wrq)
			.mode(Mode::Octet)
			.filename(&filename);

		let mut options = options.to_owned();
		if options.len() > 0 {
			if let Some(i) = options.iter().position(|e| e.kind() == TftpOptionKind::TransferSize) {
				options[i] = TftpOption::TransferSize(file.metadata().unwrap().len() as u32);
			}
			builder = builder.options(&options[..]);
		}
		let pkt = builder.build();
		conn.send_request_to(&pkt, server);

		let mut buf = [0u8; 64];
		match conn.receive_packet_from(&mut buf, None) {
			Ok((pkt, remote)) if pkt.packet_kind() == PacketKind::OAck => {
				if remote.ip() != server.ip() {
					return conn.drop();
				}
				conn.connect_to(remote).unwrap();

				let TftpPacket::OAck(oack) = pkt else { unreachable!() };
				let opts = tftp::options::parse_tftp_options(oack.options().unwrap()).unwrap();
				conn.set_options(&opts[..]);

				tftp::send_data(conn, file).await;
			},
			Ok((pkt, remote)) if pkt.packet_kind() == PacketKind::Ack => {
				if remote.ip() != server.ip() {
					return conn.drop();
				}
				conn.connect_to(server).unwrap();
				tftp::send_data(conn, file).await
			},
			Ok((pkt, _)) => error!("Received packet of unexpected kind {}", pkt.packet_kind()),
			Err(e) => error!("Server didn't properly respond to request ({})", e),
		}
	}
}

pub async fn run_client(action: cli::ClientAction, opts: cli::ClientOpts, cxl_token: CancellationToken) -> Result<(), String> {
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