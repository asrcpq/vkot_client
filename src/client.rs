use std::io::{BufWriter, Read, Write, Result};
use std::os::unix::net::UnixStream;

use crate::msg::ServerMsg;

pub struct Client {
	writer: BufWriter<UnixStream>,
	stream: UnixStream,
	buf: [u8; 1024],
}

impl Default for Client {
	fn default() -> Self {
		let s: String = match std::env::var("VKOT_SOCKET") {
			Ok(val) => val,
			Err(_) => "./vkot.socket".to_string(),
		};
		let stream = UnixStream::connect(s).unwrap();
		let writer = BufWriter::new(stream.try_clone().unwrap());
		Self {
			writer,
			stream,
			buf: [0u8; 1024],
		}
	}
}

impl Client {
	pub fn poll_event(&mut self) -> Option<ServerMsg> {
		let len = match self.stream.read(&mut self.buf) {
			Ok(l) => l,
			Err(_) => return None,
		};
		if len == 0 { return None }
		// temporary parser
		let string = String::from_utf8_lossy(&self.buf[..len]);
		let ch = string.chars().next().unwrap();
		Some(ServerMsg::Getch(ch))
	}

	pub fn clear(&mut self) -> Result<()> {
		self.writer.write(&[b'C'])?;
		Ok(())
	}

	pub fn print(&mut self, msg: &str) -> Result<()> {
		self.writer.write(&[b'p'])?;
		self.writer.write(&(msg.len() as u32).to_le_bytes())?;
		self.writer.write(msg.as_bytes())?;
		Ok(())
	}

	pub fn set_color(&mut self, color: [f32; 4]) -> Result<()> {
		self.writer.write(&[b'c']).unwrap();
		for c in color.iter() {
			self.writer.write(&c.to_le_bytes())?;
		}
		Ok(())
	}

	pub fn move_cursor(&mut self, pos: [u32; 2]) -> Result<()> {
		self.writer.write(&[b'm'])?;
		self.writer.write(&pos[0].to_le_bytes())?;
		self.writer.write(&pos[1].to_le_bytes())?;
		Ok(())
	}

	pub fn flush(&mut self) -> Result<()> {
		self.writer.flush()
	}
}
