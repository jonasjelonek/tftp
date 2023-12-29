use std::io;
use std::net::{UdpSocket, SocketAddr, IpAddr};
use std::fs::OpenOptions;
use std::time::Duration;
use std::collections::HashMap;

use tokio_util::sync::CancellationToken;

#[allow(unused)]
use log::{info, warn, error, debug, trace};

use crate::tftp::{
	self,
	/* submodules */
	options::*,
	packet as pkt,

	Mode, RequestKind, TftpConnection
};

// ############################################################################
// ############################################################################
// ############################################################################

pub struct TftpServer {
	listen_addr: IpAddr,
	cancel_token: CancellationToken,
}

// ############################################################################
// ############################################################################
// ############################################################################

impl TftpServer {

	pub fn new(local_ip: IpAddr, cancel_token: CancellationToken) -> Self {
		TftpServer { 
			listen_addr: local_ip,
			cancel_token,
		}
	}

	async fn negotiate_options<'a>(&self,
		conn: &mut TftpConnection,
		raw_opts: HashMap<&'a str, &'a str>,
		transfer_size: u32,
		req_kind: RequestKind
	) -> Result<bool, OptionError> {
		if raw_opts.is_empty() {
			return Ok(false);
		}
		
		let mut negotiation = OptionNegotiation
			::parse_options(raw_opts)
			.map_err(|_| OptionError::InvalidOption)?;

		// Set transfer size if client requested it
		if req_kind == RequestKind::Rrq {
			if let Some(opt) = negotiation.find_option_mut(TftpOptionKind::TransferSize) &&
			   let TftpOption::TransferSize(tsz) = opt &&
			   *tsz == 0
			{ *tsz = transfer_size; }
		} else {
			// Check if enough space is available
		}

		let oack_pkt = pkt::MutableTftpPacket::OAck(negotiation.build_oack_packet());
		conn.send_packet(&oack_pkt);

		if req_kind == RequestKind::Rrq {
			let mut buf: [u8; 16] = [0; 16];

			conn.receive_packet(&mut buf[..], Some(pkt::PacketKind::Ack))
				.map_err(|_| OptionError::NoAck)?;
		}
		
		conn.options_mut().merge_from(&negotiation);
		Ok(true)
	}

	pub async fn handle_request<'a>(&self, req: pkt::TftpReq<'a>, client: SocketAddr) {
		let mut conn = match TftpConnection::new(
			self.listen_addr, client,
			TftpOptions::default(),
			self.cancel_token.clone()
		) {
			Ok(con) => con,
			Err(e) => return error!("failed to handle request due to lower layer error: {}", e),
		};

		match req.mode() {
			Ok(mode) if mode == Mode::Octet => (),
			Ok(mode) if mode == Mode::NetAscii => {
				return conn.drop_with_err(
					tftp::ErrorCode::NotDefined, 
					Some("NetAscii mode not supported".to_string())
				)
			},
			Ok(_) | Err(_) => {
				return conn.drop_with_err(
					tftp::ErrorCode::NotDefined, 
					Some("Malformed request; invalid mode".to_string())
				)
			},
		}
	
		let mut path = crate::working_dir().clone();
		let Ok(filename) = req.filename() else {
			return conn.drop_with_err(
				tftp::ErrorCode::NotDefined, 
				Some("Malformed request; missing filename".to_string())
			);
		};
		path.push(filename);

		let mut file_opts = OpenOptions::new();
		match req.kind() {
			RequestKind::Rrq => file_opts.read(true),
			RequestKind::Wrq => file_opts.create(true).truncate(true).write(true),
		};

		let file = match file_opts.open(&path) {
			Ok(f) => f,
			Err(e) if e.kind() == io::ErrorKind::NotFound => return conn.drop_with_err(tftp::ErrorCode::FileNotFound, None),
			Err(e) if e.kind() == io::ErrorKind::PermissionDenied => return conn.drop_with_err(tftp::ErrorCode::AccessViolation, None),
			Err(e) => return conn.drop_with_err(tftp::ErrorCode::StorageError, Some(e.to_string())),
		};
		let file_len = match req.kind() {
			RequestKind::Wrq => 0,
			RequestKind::Rrq => file.metadata().unwrap().len() as u32,
		};

		/* Read, parse and acknowledge/reject options requested by the client. */
		match self.negotiate_options(&mut conn, req.options().unwrap(), file_len, req.kind()).await {
			Err(_) => return conn.drop(),
			Ok(true) => (),
			Ok(false) => {
				let inner_ack = pkt::MutableTftpAck::new(0);
				let wrq_ack = pkt::MutableTftpPacket::Ack(inner_ack);
				conn.send_packet(&wrq_ack);
			}
		};
		conn.set_reply_timeout(conn.opt_timeout());
		conn.set_tx_mode(req.mode().unwrap());
	
		info!("{:?} from {}", req.kind(), conn.peer());
		match req.kind() {
			tftp::RequestKind::Rrq => tftp::send_file(conn, file).await,
			tftp::RequestKind::Wrq => tftp::receive_file(conn, file).await,
		};
	}
}

pub async fn run_server(listen_addr: SocketAddr, cxl_token: CancellationToken) -> Result<(), String> {
	let socket = UdpSocket::bind(listen_addr).unwrap();
	socket.set_read_timeout(Some(Duration::from_millis(500))).unwrap();

	loop {
		if cxl_token.is_cancelled() {
			break Ok(());
		}

		/* this buffer will be moved into the task below */
		let mut recv_buf = Box::new([0; 128]);
		match socket.recv_from(recv_buf.as_mut()) {
			Ok((size, client)) => {
				debug!("received packet of size {} from {}", size, client);

				let task_cxl_token = cxl_token.clone();
				tokio::spawn(async move {
					let Ok(packet) = pkt::TftpReq::try_from_buf(&recv_buf[..size]) else {
						return error!("only TFTP requests accepted on this socket (client: {})", client);
					};
					TftpServer
						::new(listen_addr.ip(), task_cxl_token)
						.handle_request(packet, client).await;
				});
			},
			Err(e) => {
				match e.kind() {
					std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => (),
					_ => error!("{}", e)
				}
			}
		}
	}
}