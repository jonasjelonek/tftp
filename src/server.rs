use std::error::Error;
use std::io;
use std::net::{UdpSocket, SocketAddr, IpAddr};
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::time::Duration;
use std::collections::HashMap;

use tokio_util::sync::CancellationToken;

#[allow(unused)]
use log::{info, warn, error, debug, trace};

use crate::tftp::error::{ErrorCode, OptionError, RequestError};
use crate::tftp::{RequestKind, TftpConnection};
use crate::tftp::options::{parse_tftp_options, TftpOption, TftpOptionKind};
use crate::tftp::packet as pkt;

// ############################################################################
// ############################################################################
// ############################################################################

pub type Result<T> = std::result::Result<T, RequestError>;

pub struct TftpRequestHandler {
	listen_addr: IpAddr,
	cancel_token: CancellationToken,
	root: PathBuf,
}

// ############################################################################
// ############################################################################
// ############################################################################

impl TftpRequestHandler {

	pub fn new(local_ip: IpAddr, root: PathBuf, cancel_token: CancellationToken) -> Self {
		TftpRequestHandler { 
			listen_addr: local_ip,
			cancel_token,
			root
		}
	}

	async fn negotiate_options<'a>(&self,
		conn: &mut TftpConnection,
		raw_opts: HashMap<&'a str, &'a str>,
		transfer_size: u32,
		req_kind: RequestKind
	) -> Result<bool> {
		if raw_opts.is_empty() {
			return Ok(false);
		}

		let mut requested_options = parse_tftp_options(raw_opts)?;

		// Set transfer size if client requested it
		if req_kind == RequestKind::Rrq {
			if let Some(tf_size) = requested_options.iter_mut().find(|e| e.kind() == TftpOptionKind::TransferSize) {
				*tf_size = TftpOption::TransferSize(transfer_size);
			}
		} else {
			// TODO: Check if enough space is available
		}

		let oack_pkt = pkt::builder::TftpOAckBuilder
			::new()
			.options(&requested_options[..])
			.build();
		conn.send_packet(&oack_pkt)?;

		if req_kind == RequestKind::Rrq {
			let mut buf: [u8; 16] = [0; 16];

			match conn.receive_packet(&mut buf[..]) {
				Ok(pkt::TftpPacket::Ack(_)) => (),
				Ok(_) => return Err(OptionError::NoAck.into()),
				Err(e) => return Err(e.into())
			}
		}
		
		conn.set_options(&requested_options[..]);
		Ok(true)
	}

	pub async fn handle_request<'a>(&self, req: pkt::TftpReq<'a>, client: SocketAddr) -> Result<()> {
		let mut conn = TftpConnection::new(
			self.listen_addr,
			self.cancel_token.clone()
		)?;
		conn.connect_to(client)?;

		match req.mode() {
			Ok(mode) => conn.set_tx_mode(mode)?, 
			Err(_) => {
				conn.send_error(ErrorCode::NotDefined, "Malformed request; invalid mode").ok();
				return Err(RequestError::MalformedRequest);
			},
		}
	
		let mut path = self.root.clone();
		let Ok(filename) = req.filename() else {
			conn.send_error(ErrorCode::NotDefined, "Malformed request; missing filename").ok();
			return Err(RequestError::MalformedRequest);
		};
		path.push(filename);

		let mut file_opts = OpenOptions::new();
		match req.kind() {
			RequestKind::Rrq => file_opts.read(true),
			RequestKind::Wrq => file_opts.create(true).truncate(true).write(true),
		};

		let file = match file_opts.open(&path) {
			Ok(f) => f,
			Err(e) if e.kind() == io::ErrorKind::NotFound => {
				conn.send_error(ErrorCode::FileNotFound, "").ok();
				return Err(RequestError::FileNotFound);
			},
			Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
				conn.send_error(ErrorCode::AccessViolation, "").ok();
				return Err(RequestError::FileNotAccessible);
			},
			Err(e) => {
				conn.send_error(ErrorCode::StorageError, e.to_string().as_str()).ok();
				return Err(RequestError::OtherHostError(e));
			},
		};
		let file_len = match req.kind() {
			RequestKind::Wrq => 0,
			RequestKind::Rrq => file.metadata().unwrap().len() as u32,
		};

		/* Read, parse and acknowledge/reject options requested by the client. */
		if self.negotiate_options(&mut conn, req.options().unwrap(), file_len, req.kind()).await? == false {
			if req.kind() == RequestKind::Wrq {
				let wrq_ack = pkt::MutableTftpAck::new(0);
				conn.send_packet(&wrq_ack)?;
			}
			conn.set_reply_timeout(conn.opt_timeout());
		}
	
		info!("{:?} from {}", req.kind(), conn.peer());
		match req.kind() {
			RequestKind::Rrq => conn.send_data(file).await?,
			RequestKind::Wrq => conn.receive_data(file, None).await?,
		};
		Ok(())
	}
}

pub struct TftpServer {
	listen_addr: SocketAddr,
	socket: UdpSocket,
	root: PathBuf,
}
impl TftpServer {

	pub fn new(listen_addr: SocketAddr, root: PathBuf) -> std::result::Result<Self, Box<dyn Error>> {
		let socket = UdpSocket::bind(listen_addr)?;
		socket.set_read_timeout(Some(Duration::from_millis(500)))?;

		Ok(Self { listen_addr, socket, root })
	}

	pub async fn run(&self, cxl_token: CancellationToken) -> Result<()> {
		loop {
			if cxl_token.is_cancelled() {
				warn!("Server task cancelled by signal");
				break;
			}

			/* this buffer will be moved into the task below */
			let mut recv_buf = Box::new([0; 128]);
			match self.socket.recv_from(recv_buf.as_mut()) {
				Ok((size, client)) => {
					debug!("received packet ({} bytes) from {}", size, client);
	
					let task_cxl_token = cxl_token.clone();
					let listen_addr = self.listen_addr.ip();
					let root_dir = self.root.clone();
					tokio::spawn(async move {
						let Ok(packet) = pkt::TftpReq::try_from(&recv_buf[..size]) else {
							return error!("only TFTP requests accepted on this socket (client: {})", client);
						};
						let _ = TftpRequestHandler
							::new(listen_addr, root_dir, task_cxl_token)
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
		Ok(())
	}
}