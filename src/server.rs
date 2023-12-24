use std::net::{UdpSocket, SocketAddr, IpAddr};
use std::fs::OpenOptions;
use std::io::{Read, BufReader};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

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
	cancel_token: CancellationToken
}

// ############################################################################
// ############################################################################
// ############################################################################

const EMPTY_CHUNK: &[u8] = &[];

impl TftpServer {

	pub fn new(listen_addr: IpAddr, cancel_token: CancellationToken) -> Self {
		TftpServer { 
			listen_addr,
			cancel_token
		}
	}

	///
	/// send_file
	/// 
	/// This should be able to be used in RRQ in server mode and WRQ in client mode
	async fn send_file(&self, mut conn: TftpConnection) {
		let peer = conn.peer();
		let filename = conn.file_path().file_name().unwrap().to_owned();
	
		/* This is safe, however it leaves None in conn.file_handle. Maybe do this another way? */
		/* Probably already store a BufReader/BufWriter in the connection instead of the file */
		let file_handle = conn.take_file_handle();
		let filesize = file_handle.metadata().unwrap().len();
		let blocksize = conn.opt_blocksize();
	
		info!("RRQ from {} to serve '{}' in '{}' mode", peer, filename.to_string_lossy(), conn.tx_mode());
		if conn.tx_mode() == Mode::NetAscii {
			return conn.drop_with_err(
				tftp::ErrorCode::IllegalOperation, 
				Some(format!("NetAscii not supported"))
			);
		}
	
		let mut file_read = BufReader::new(file_handle);
		debug!("start sending file");
	
		// Let's only use one buffer for file reading and packet sending, we just always keep the first 4 bytes reserved
		// for packet header and read the file block after it.
		let mut file_buf: Vec<u8> = vec![0; 4 + (blocksize as usize)];
		let mut n_blocks = filesize / (blocksize as u64);
		let remainder = filesize % (blocksize as u64);
		if remainder != 0 {
			/* add one for remaining bytes */
			n_blocks += 1;
		}
	
		for blocknum in 1..=n_blocks {
			if conn.host_cancelled() {
				conn.drop();
				return;
			}
	
			file_buf.truncate(4);
			if let Err(e) = file_read.by_ref().take(blocksize as u64).read_to_end(&mut file_buf) {
	
			}
			trace!("file_buf has len {} and capacity {}", file_buf.len(), file_buf.capacity());
	
			let mut pkt = pkt::MutableTftpData::try_from(&mut file_buf[..], true).unwrap();
			pkt.set_blocknum(blocknum as u16);
			
			let pkt_ref = pkt::MutableTftpPacket::Data(pkt);
			match conn.send_and_wait_for_reply(&pkt_ref, pkt::PacketKind::Ack, 5) {
				Ok(_) => (),
				Err(_e) => return conn.drop(),
			}
		}

		/* send a final empty packet in case file_len mod blocksize == 0 */
		if remainder == 0 {
			file_buf.truncate(4);
			
			let mut pkt = pkt::MutableTftpData::try_from(&mut file_buf[..], false).unwrap();
			pkt.set_blocknum(pkt.blocknum() + 1);
			pkt.set_data(EMPTY_CHUNK);
			
			let pkt_ref = pkt::MutableTftpPacket::Data(pkt);
			match conn.send_and_wait_for_reply(&pkt_ref, pkt::PacketKind::Ack, 5) {
				Ok(_) => (),
				Err(_e) => return conn.drop(),
			}
		}
	
		debug!("served file");
	}

	/*
	async fn receive_file<'a>(req: pkt::TftpReq<'a>, socket: &UdpSocket, cxl_token: CancellationToken) -> Result<(), Error> {
		let filename = req.filename()?;
		let mode = req.mode()?;
		let peer = socket.peer_addr().unwrap();

		info!("WRQ from {} to receive '{}' in '{}' mode", peer, filename, mode);
		if mode == Mode::NetAscii {
			return Err(tftp_err!(IllegalOperation, Some(format!("NetAscii not supported"))));
		}
		
		// create/overwrite file
		// send ACK to client
		// loop: receive DATA and send ACK

		Ok(())
	} */

	pub async fn handle_request<'a>(&self, req: pkt::TftpReq<'a>, from: SocketAddr) {
		let socket = match UdpSocket::bind(SocketAddr::new(self.listen_addr, 0)) {
			Ok(s) => s,
			Err(e) => return error!("failed to open Udp socket for TFTP operation: {}", e),
		};
		socket.connect(from).unwrap();
	
		/* 
		 * create a connection without a file, it will be added later. This way we already have
		 * the drop functionality of the connection in case the file is not accessible.
		 */
		let mut conn = TftpConnection::new(
			socket, 
			TftpOptions::default(),
			req.kind(), 
			self.cancel_token.clone()
		);
	
		/* Build file path and check if file is acessible with respect to the request kind we handle here */
		let mut file_opts = OpenOptions::new();
		match req.kind() {
			RequestKind::Rrq => file_opts.read(true),
			RequestKind::Wrq => file_opts.create(true).truncate(true).write(true),
		};
	
		let mut path = crate::working_dir().clone();
		let Ok(filename) = req.filename() else {
			return conn.drop_with_err(
				tftp::ErrorCode::NotDefined, 
				Some("Malformed request; missing filename".to_string())
			);
		};
		path.push(filename);
	
		/* 
		 * add the opened file to our connection, it is either an existing file to read (in case of RRQ)
		 * or a new or existing truncated file to write to (see OpenOptions above!)
		 */
		match file_opts.open(&path) {
			Ok(f) => conn.set_file_handle(f),
			Err(e) => {
				match e.kind() {
					std::io::ErrorKind::NotFound => return conn.drop_with_err(tftp::ErrorCode::FileNotFound, None),
					std::io::ErrorKind::PermissionDenied => return conn.drop_with_err(tftp::ErrorCode::AccessViolation, None),
					_ => return conn.drop_with_err(tftp::ErrorCode::StorageError, Some(e.to_string())),
				}
			}
		}
		conn.set_file_path(&path);
	
		/* Read, parse and acknowledge/reject options requested by the client. */
		conn.negotiate_options(req.options().unwrap()).unwrap();
		conn.set_reply_timeout(conn.opt_timeout());
	
		match req.kind() {
			tftp::RequestKind::Rrq => self.send_file(conn).await,
			tftp::RequestKind::Wrq => (),//self.receive_file(conn).await,
		};
	}
}

pub async fn server_task(listen_addr: SocketAddr, cxl_token: CancellationToken) -> Result<(), String> {
	let socket = UdpSocket::bind(listen_addr).unwrap();
	socket.set_read_timeout(Some(Duration::from_millis(500))).unwrap();

	loop {
		if cxl_token.is_cancelled() {
			break Ok(());
		}

		/* this will be moved into the task below */
		let mut recv_buf = Box::new([0; 128]);
		match socket.recv_from(recv_buf.as_mut()) {
			Ok((size, client)) => {
				debug!("received packet of size {} from {}", size, client);

				let task_cxl_token = cxl_token.clone();
				tokio::spawn(async move {
					if let Ok(packet) = pkt::TftpReq::try_from_buf(&recv_buf[..size]) {
						let server = TftpServer::new(
							listen_addr.ip(),
							task_cxl_token
						);
						server.handle_request(packet, client).await;
					} else {
						return error!("only TFTP requests accepted on this socket (client: {})", client);
					}
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