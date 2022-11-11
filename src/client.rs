use std::collections::VecDeque;
use std::io::{BufWriter, Read, Write, Result};
use std::os::unix::net::UnixStream;

use crate::msg::ServerMsg;

pub fn wide_test(ch: char) -> bool {
	match unicode_width::UnicodeWidthChar::width(ch) {
		Some(x) => x >= 2,
		None => true,
	}
}

pub struct WriteHalf {
	writer: BufWriter<UnixStream>,
	buffer: Vec<Vec<(u32, u32)>>,
	size: [i16; 2],
	damage: [i16; 4],
	cursor: [i16; 2],
	current_color: u32,
}

impl WriteHalf {
	pub fn new(stream: UnixStream) -> Self {
		Self {
			writer: BufWriter::new(stream),
			buffer: vec![vec![
				(b' ' as u32, u32::MAX); 80
			]; 24],
			size: [80, 24],
			damage: [0; 4],
			cursor: [0; 2],
			current_color: u32::MAX,
		}
	}

	pub fn clear(&mut self) {
		let sx = self.size[0] as usize;
		let sy = self.size[1] as usize;
		self.buffer = vec![vec![
			(b' ' as u32, u32::MAX); sx
		]; sy];
	}

	pub fn reset(&mut self) {
		self.clear();
		self.cursor = [0; 2];
		self.flush().unwrap();
	}

	pub fn print(&mut self, ch: char) {
		if ch == '\n' {
			self.cursor[0] = 0;
			self.cursor[1] += 1;
			return
		}
		let cx = self.cursor[0] as usize;
		let cy = self.cursor[1] as usize;
		if wide_test(ch) {
			self.loc(2, 2);
		} else {
			self.loc(2, 1);
		}
		let ch = ch as u32;
		self.buffer[cy][cx] = (ch, self.current_color);
	}

	// pub fn resize()

	pub fn set_color(&mut self, color: u32) {
		self.current_color = color;
	}

	pub fn loc(&mut self, ty: u8, pos: i16) {
		match ty {
			0 => self.cursor[0] = pos,
			1 => self.cursor[1] = pos,
			2 => self.cursor[0] += pos,
			3 => self.cursor[1] += pos,
			_ => panic!(),
		}
	}

	pub fn flush(&mut self) -> Result<()> {
		// FIXME: damage
		self.writer.write(&[0])?;
		self.writer.write(&0i16.to_le_bytes())?;
		self.writer.write(&0i16.to_le_bytes())?;
		self.writer.write(&self.size[0].to_le_bytes())?;
		self.writer.write(&self.size[1].to_le_bytes())?;
		for line in self.buffer.iter() {
			for cell in line.iter() {
				self.writer.write(&cell.0.to_le_bytes())?;
				self.writer.write(&cell.1.to_le_bytes())?;
			}
		}
		self.writer.flush()?;
		Ok(())
	}
}

pub struct ReadHalf {
	stream: UnixStream,
	buf: [u8; 32768],
	event_queue: VecDeque<ServerMsg>,
}

impl ReadHalf {
	pub fn new(stream: UnixStream) -> Self {
		Self {
			stream,
			buf: [0; 32768],
			event_queue: VecDeque::new(),
		}
	}

	pub fn poll_event(&mut self) -> Option<ServerMsg> {
		if !self.event_queue.is_empty() {
			return self.event_queue.pop_front();
		}
		let len = match self.stream.read(&mut self.buf) {
			Ok(l) => l,
			Err(_) => {
				eprintln!("Read error");
				return None
			}
		};
		if len == 0 {
			eprintln!("EOF");
			return None
		}
		let result = match ServerMsg::from_buf(&self.buf[..len], &mut 0) {
			Ok(x) => x,
			Err(e) => {
				eprintln!("{:?}", e);
				return None
			}
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
