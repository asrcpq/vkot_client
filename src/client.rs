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

const ECELL: (u32, u32) = (b' ' as u32, u32::MAX);

pub struct WriteHalf {
	writer: BufWriter<UnixStream>,
	buffer: Vec<Vec<(u32, u32)>>,
	size: [i16; 2],
	_damage: [i16; 4],
	cursor: [i16; 2],
	current_color: u32,
	eol: bool,
}

impl WriteHalf {
	pub fn new(stream: UnixStream) -> Self {
		Self {
			writer: BufWriter::new(stream),
			buffer: vec![vec![ECELL; 80]; 24],
			size: [80, 24],
			_damage: [0; 4],
			cursor: [0; 2],
			current_color: u32::MAX,
			eol: false,
		}
	}

	pub fn resize(&mut self, new_size: [i16; 2]) {
		eprintln!("resizing to {:?}", new_size);
		self.buffer.resize(new_size[1] as usize, vec![ECELL; new_size[0] as usize]);
		for line in self.buffer.iter_mut() {
			line.resize(new_size[0] as usize, ECELL);
		}
		self.size = new_size;
	}

	pub fn clear(&mut self) {
		let sx = self.size[0] as usize;
		let sy = self.size[1] as usize;
		self.buffer = vec![vec![ECELL; sx]; sy];
	}

	pub fn reset(&mut self) {
		self.clear();
		self.cursor = [0; 2];
		self.refresh().unwrap();
	}

	pub fn tab(&mut self) {
		let cx = self.cursor[0] as usize;
		let target = (cx / 8 + 1) * 8;
		if target >= self.size[0] as usize {
			return
		}
		for i in cx..target {
			self.buffer[self.cursor[1] as usize][i] = ECELL;
		}
		self.cursor[0] = target as i16;
	}

	pub fn fixcur(&mut self) {
		if self.cursor[0] < 0{
			self.cursor[0] = 0;
		}
		if self.cursor[1] < 0{
			self.cursor[1] = 0;
		}
		// TODO: prevent crash/loop for 1/0(because of pending eol) size
		while self.cursor[0] > self.size[0] {
			self.cursor[1] += 1;
			self.cursor[0] -= self.size[0]
		}
		if self.cursor[0] == self.size[0] && !self.eol {
			self.eol = true;
		}
		// simple wrapping
		while self.cursor[1] >= self.size[1] {
			self.cursor[1] -= self.size[1];
		}
	}

	fn print(&mut self, ch: char) {
		match ch {
			'\n' => {
				self.cursor[0] = 0;
				self.cursor[1] += 1;
				self.fixcur();
				return
			}
			_ => {
				let cx = self.cursor[0] as usize;
				let cy = self.cursor[1] as usize;
				let chu = ch as u32;
				self.buffer[cy][cx] = (chu, self.current_color);
			}
		}
	}

	pub fn shift_by_width(&mut self, ch: char) {
		if wide_test(ch) {
			self.loc(2, 2);
		} else {
			self.loc(2, 1);
		}
	}

	pub fn put(&mut self, ch: char, shift: bool) -> Result<()> {
		if self.eol {
			self.loc(2, 1);
		}
		self.print(ch);
		self.writer.write(&[1])?;
		self.writer.write(&self.cursor[0].to_le_bytes())?;
		self.writer.write(&self.cursor[1].to_le_bytes())?;
		self.writer.write(&(ch as u32).to_le_bytes())?;
		self.writer.write(&self.current_color.to_le_bytes())?;
		if shift { self.shift_by_width(ch); }
		self.send_cursor()?;
		self.writer.flush()?;
		Ok(())
	}

	pub fn erase_display(&mut self, code: u16) {
		eprintln!("edisp");
		let [begin, end] =match code {
			0 => {
				self.erase_line(0);
				[self.cursor[1] + 1, self.size[1]]
			}
			1 => {
				self.erase_line(1);
				[0, self.cursor[1]]
			}
			_ => [0, self.size[1]],
		};
		for row in begin..end {
			self.buffer[row as usize] = vec![ECELL; self.size[0] as usize];
		}
	}

	pub fn erase_line(&mut self, code: u16) {
		eprintln!("eline");
		let [begin, end] =match code {
			0 => [self.cursor[0], self.size[0]],
			1 => [0, self.cursor[0] + 1],
			_ => [0, self.size[0]],
		};
		let row = &mut self.buffer[self.cursor[1] as usize];
		for col in begin..end {
			row[col as usize] = ECELL;
		}
	}

	pub fn set_color(&mut self, color: u32) {
		self.current_color = color;
	}

	pub fn loc(&mut self, ty: u8, pos: i16) {
		self.eol = false;
		match ty {
			0 => self.cursor[0] = pos,
			1 => self.cursor[1] = pos,
			2 => self.cursor[0] += pos,
			3 => self.cursor[1] += pos,
			_ => panic!(),
		}
		self.fixcur();
		self.send_cursor().unwrap();
		self.writer.flush().unwrap();
	}

	pub fn send_area(&mut self, area: [i16; 4]) -> Result<()> {
		self.writer.write(&[0])?;
		self.writer.write(&area[0].to_le_bytes())?;
		self.writer.write(&area[1].to_le_bytes())?;
		self.writer.write(&area[2].to_le_bytes())?;
		self.writer.write(&area[3].to_le_bytes())?;
		for line in self.buffer.iter() {
			for cell in line.iter() {
				self.writer.write(&cell.0.to_le_bytes())?;
				self.writer.write(&cell.1.to_le_bytes())?;
			}
		}
		Ok(())
	}

	pub fn send_cursor(&mut self) -> Result<()> {
		self.writer.write(&[2])?;
		self.writer.write(&self.cursor[0].to_le_bytes())?;
		self.writer.write(&self.cursor[1].to_le_bytes())?;
		Ok(())
	}

	pub fn refresh(&mut self) -> Result<()> {
		self.send_area([0, 0, self.size[0], self.size[1]])?;
		self.send_cursor()?;
		self.writer.flush()?;
		Ok(())
	}

	pub fn send_damage(&mut self) -> Result<()> {
		// FIXME
		self.refresh()
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
