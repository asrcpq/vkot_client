use std::collections::VecDeque;
use std::io::{BufWriter, Read, Write, Result};
use std::os::unix::net::UnixStream;

use crate::msg::ServerMsg;
use vkot_common::cell::Cell;
use vkot_common::region::Region;

pub fn wide_test(ch: char) -> (bool, i16) {
	let wide = match unicode_width::UnicodeWidthChar::width(ch) {
		Some(x) => x >= 2,
		None => true,
	};
	if wide {
		(true, 2)
	} else {
		(false, 1)
	}
}

pub struct WriteHalf {
	writer: BufWriter<UnixStream>,
	history: VecDeque<Vec<Cell>>,
	histcur: usize,
	buffer: Vec<Vec<Cell>>,
	// current empty cell
	ecell: Cell,
	reversed: bool,
	// all in x, y(or col, row) order
	size: [i16; 2],
	damage: Region,
	cursor: [i16; 2],
	eol: bool,
}

impl WriteHalf {
	pub fn new(stream: UnixStream) -> Self {
		Self {
			writer: BufWriter::new(stream),
			history: VecDeque::new(),
			histcur: 0,
			buffer: vec![vec![Cell::default(); 80]; 24],
			ecell: Cell::default(),
			reversed: false,
			size: [80, 24],
			damage: Region::default(),
			cursor: [0; 2],
			eol: false,
		}
	}

	pub fn resize(&mut self, new_size: [i16; 2]) {
		eprintln!("resizing to {:?}", new_size);
		self.buffer.resize(new_size[1] as usize, vec![self.ecell; new_size[0] as usize]);
		for line in self.buffer.iter_mut() {
			line.resize(new_size[0] as usize, self.ecell);
		}
		self.size = new_size;
	}

	pub fn clear(&mut self) {
		let sx = self.size[0] as usize;
		let sy = self.size[1] as usize;
		self.buffer = vec![vec![self.ecell; sx]; sy];
	}

	pub fn reset(&mut self) {
		self.clear();
		self.loc(0, 0);
		self.loc(1, 0);
		self.refresh().unwrap();
	}

	pub fn refresh(&mut self) -> Result<()> {
		self.damage_all();
		self.send_damage()
	}

	pub fn tab(&mut self) {
		let cx = self.cursor[0] as usize;
		let target = (cx / 8 + 1) * 8;
		if target >= self.size[0] as usize {
			return
		}
		for i in cx..target {
			self.buffer[self.cursor[1] as usize][i] = self.ecell;
		}
		let target = target as i16;
		self.loc(0, target);
	}

	fn limit_cursor(&mut self) -> bool {
		let mut result = false;
		if self.cursor[0] < 0{
			self.cursor[0] = 0;
			result = true;
		}
		if self.cursor[1] < 0{
			self.cursor[1] = 0;
			result = true;
		}
		if self.cursor[0] >= self.size[0] {
			self.cursor[0] = self.size[0] - 1;
			result = true;
		}
		if self.cursor[1] >= self.size[1] {
			self.cursor[1] = self.size[1] - 1;
			result = true;
		}
		result
	}

	pub fn scroll(&mut self, down: bool) {
		self.damage_all();
		if down {
			self.buffer.push(vec![self.ecell; self.size[0] as usize]);
			self.history.push_front(self.buffer.remove(0));
			let hlen = self.history.len();
			if hlen > 10000 {
				self.history.drain(10001..);
			}
		} else {
			self.buffer.insert(0, vec![self.ecell; self.size[0] as usize]);
			self.buffer.pop();
		}
	}

	pub fn scroll_history_page(&mut self, down: bool) {
		let dy = self.size[1] as usize / 2;
		if down {
			self.histcur = self.histcur.saturating_sub(dy)
		} else {
			self.histcur += dy;
			self.histcur = self.histcur.min(self.history.len());
		}
		self.damage_all();
	}

	pub fn put(&mut self, ch: char) {
		debug_assert!(!ch.is_ascii_control());
		let (wide, width) = wide_test(ch);
		if self.eol {
			self.eol = false;
			self.newline();
		}

		if self.cursor[0] == self.size[0] - 1 && wide {
			self.newline();
		}

		let new_eol = self.cursor[0] == self.size[0] - width;
		let cx = self.cursor[0] as usize;
		let cy = self.cursor[1] as usize;
		let unic = ch as u32;
		self.buffer[cy][cx] = self.ecell.with_unic(unic);
		self.include_damage(Region::new([
			self.cursor[0],
			self.cursor[1],
			self.cursor[0] + 1,
			self.cursor[1] + 1,
		]));
		if !new_eol {
			// if wide {
			// 	self.put(' '); // overwrite another space
			// }
			// self.loc(2, 1);
			self.loc(2, width); // FIXME
		} else {
			self.eol = true;
		}
	}

	pub fn erase_display(&mut self, code: u16, send: bool) -> Result<()> {
		let [begin, end] =match code {
			0 => {
				self.erase_line(0, true)?;
				[self.cursor[1] + 1, self.size[1]]
			}
			1 => {
				self.erase_line(1, true)?;
				[0, self.cursor[1]]
			}
			_ => [0, self.size[1]],
		};
		for row in begin..end {
			self.buffer[row as usize] = vec![self.ecell; self.size[0] as usize];
		}
		if send {
			self.include_damage(Region::new(
				[0, begin, self.size[1], end]
			));
		}
		Ok(())
	}

	pub fn erase_line(&mut self, code: u16, send: bool) -> Result<()> {
		let [begin, end] =match code {
			0 => [self.cursor[0], self.size[0]],
			1 => [0, self.cursor[0] + 1],
			_ => [0, self.size[0]],
		};
		let row = &mut self.buffer[self.cursor[1] as usize];
		for col in begin..end {
			row[col as usize] = self.ecell;
		}
		if send {
			self.include_damage(Region::new(
				[begin, self.cursor[1], end, self.cursor[1] + 1]
			));
		}
		Ok(())
	}

	pub fn fg_color(&mut self, color: u32) {
		self.ecell.fg = color;
	}

	pub fn bg_color(&mut self, color: u32) {
		self.ecell.bg = color;
	}

	pub fn reverse_color(&mut self, reversed: bool) {
		self.reversed = reversed;
	}

	pub fn newline(&mut self) {
		if self.cursor[1] == self.size[1] - 1 {
			self.scroll(true);
		} else {
			self.loc(3, 1);
		}
		self.loc(0, 0);
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
		self.limit_cursor();
	}

	pub fn send_area(&mut self, area: Region) -> Result<()> {
		let area = area.intersect(&Region::sizebox(self.size));
		if area.is_empty() { return Ok(()) }
		self.writer.write(&[2])?;
		area.write_le_bytes(&mut self.writer)?;
		let area = area.data();

		// respect to hist
		for y in area[1] as usize..area[3] as usize {
			for x in area[0] as usize..area[2] as usize {
				//         <-----> s = 7
				// scr:    xxxxxxx
				// hst: xxxxxxx<-- h = 3
				//        ^ y = 2, out
				let cell = if y < self.histcur {
					let yy = self.histcur - y - 1;
					self.history[yy].get(x).cloned().unwrap_or(Cell::default())
				} else {
					let yy = y - self.histcur;
					self.buffer[yy][x]
				};

				cell.write_le_bytes(&mut self.writer)?;
			}
		}
		Ok(())
	}

	pub fn send_cursor(&mut self) -> Result<()> {
		self.writer.write(&[0])?;
		self.writer.write(&self.cursor[0].to_le_bytes())?;
		self.writer.write(&self.cursor[1].to_le_bytes())?;
		Ok(())
	}

	pub fn damage_all(&mut self) {
		self.damage = Region::sizebox(self.size);
	}

	pub fn include_damage(&mut self, mut damage: Region) {
		if damage.is_empty() { return }
		damage = damage.intersect(&Region::sizebox(self.size));
		self.damage = self.damage.union(&damage);
	}

	pub fn send_damage(&mut self) -> Result<()> {
		// eprintln!("send dmg {:?}", self.damage);
		self.send_area(self.damage)?;
		self.send_cursor()?;
		self.writer.flush()?;
		self.damage = Region::default();
		Ok(())
	}
}

const BUFSIZE: usize = 2 << 10;

pub struct ReadHalf {
	stream: UnixStream,
	buf: Vec<u8>,
	event_queue: VecDeque<ServerMsg>,
}

impl ReadHalf {
	pub fn new(stream: UnixStream) -> Self {
		Self {
			stream,
			buf: vec![0; BUFSIZE],
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
