use std::collections::VecDeque;
use std::io::{BufWriter, Read, Write, Result};
use std::os::unix::net::UnixStream;

use crate::msg::ServerMsg;

pub struct WriteHalf {
	writer: BufWriter<UnixStream>,
}

impl WriteHalf {
	pub fn new(stream: UnixStream) -> Self {
		Self {
			writer: BufWriter::new(stream),
		}
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

pub struct ReadHalf {
	stream: UnixStream,
	buf: [u8; 1024],
	event_queue: VecDeque<ServerMsg>,
}

impl ReadHalf {
	pub fn new(stream: UnixStream) -> Self {
		Self {
			stream,
			buf: [0; 1024],
			event_queue: VecDeque::new(),
		}
	}

	pub fn poll_event(&mut self) -> Option<ServerMsg> {
		if !self.event_queue.is_empty() {
			return self.event_queue.pop_front();
		}
		let len = match self.stream.read(&mut self.buf) {
			Ok(l) => l,
			Err(_) => return None,
		};
		if len == 0 { return None }
		let result = match ServerMsg::from_buf(&self.buf[..len], &mut 0) {
			Ok(x) => x,
			Err(_) => return None,
		};
		self.event_queue.extend(result);
		return self.event_queue.pop_front();
	}
}

pub struct Client {
	pub wh: WriteHalf,
	pub rh: ReadHalf,
}

impl Default for Client {
	fn default() -> Self {
		let s: String = match std::env::var("VKOT_SOCKET") {
			Ok(val) => val,
			Err(_) => "./vkot.socket".to_string(),
		};
		let stream = UnixStream::connect(s).unwrap();
		let stream2 = stream.try_clone().unwrap();
		Self {
			wh: WriteHalf::new(stream),
			rh: ReadHalf::new(stream2),
		}
	}
}

impl Client {
	pub fn unwrap(self) -> (ReadHalf, WriteHalf) {
		(self.rh, self.wh)
	}
}
