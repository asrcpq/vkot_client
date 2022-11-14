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
	history: VecDeque<Vec<(u32, u32)>>,
	histcur: usize,
	buffer: Vec<Vec<(u32, u32)>>,
	// all in x, y(or col, row) order
	size: [i16; 2],
	damage: [i16; 4],
	cursor: [i16; 2],
	current_color: u32,
	eol: bool,
}

impl WriteHalf {
	pub fn new(stream: UnixStream) -> Self {
		Self {
			writer: BufWriter::new(stream),
			history: VecDeque::new(),
			histcur: 0,
			buffer: vec![vec![ECELL; 80]; 24],
			size: [80, 24],
			damage: [0; 4],
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

	pub fn limit_cursor(&mut self) {
		if self.cursor[0] < 0{
			self.cursor[0] = 0;
		}
		if self.cursor[1] < 0{
			self.cursor[1] = 0;
		}
		if self.cursor[0] >= self.size[0] {
			self.cursor[0] = self.size[0] - 1;
		}
		if self.cursor[1] >= self.size[1] {
			self.cursor[1] = self.size[1] - 1;
		}
	}

	// TODO: prevent crash/loop for 1/0(because of pending eol) size
	pub fn fixcur(&mut self) {
		if self.cursor[0] < 0{
			self.cursor[0] = 0;
		}
		if self.cursor[1] < 0{
			self.cursor[1] = 0;
		}
		while self.cursor[0] > self.size[0] {
			self.cursor[1] += 1;
			self.cursor[0] -= self.size[0]
		}
		if self.cursor[0] == self.size[0] {
			if self.eol {
				self.cursor[0] = 0;
				self.cursor[1] += 1;
			} else {
				self.eol = true;
				self.cursor[0] -= 1;
			}
		}
		if self.cursor[1] >= self.size[1] {
			self.scroll(true);
			self.cursor[1] = self.size[1] - 1;
		}
	}

	pub fn scroll(&mut self, down: bool) {
		self.damage_all();
		if down {
			self.buffer.push(vec![ECELL; self.size[0] as usize]);
			self.history.push_front(self.buffer.remove(0));
			let hlen = self.history.len();
			if hlen > 10000 {
				self.history.drain(10001..);
			}
		} else {
			self.buffer.insert(0, vec![ECELL; self.size[0] as usize]);
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
		// TODO: investigate into when and where to make render sending call
		self.send_damage().unwrap();
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

	pub fn put(&mut self, ch: char, shift: bool) -> Result<()> {
		// TODO: for wide char, overwrite another cell with space
		// TODO: check write is same and do not introduce damage
		let wide = wide_test(ch);
		if self.eol {
			self.loc(2, 1, true);
		} else if wide && self.cursor[0] == self.size[0] - 1 { // wide char skip last
			if shift {
				self.newline()
			} else {
				return Ok(()) // don't print in non shift mode
			}
		}
		self.print(ch);

		self.include_damage([self.cursor[0], self.cursor[1], self.cursor[0] + 1, self.cursor[1] + 1]);
		if shift {
			if wide {
				self.loc(2, 2, true);
			} else {
				self.loc(2, 1, true);
			}
		}
		Ok(())
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
			self.buffer[row as usize] = vec![ECELL; self.size[0] as usize];
		}
		if send {
			self.include_damage([0, begin, self.size[1], end]);
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
			row[col as usize] = ECELL;
		}
		if send {
			self.include_damage([begin, self.cursor[1], end, self.cursor[1] + 1]);
		}
		Ok(())
	}

	pub fn set_color(&mut self, color: u32) {
		self.current_color = color;
	}

	pub fn newline(&mut self) {
		if self.eol {
			self.eol = false;
			self.loc(2, 1, true);
			return
		}
		self.loc(3, 1, true);
		self.loc(0, 0, false);
	}

	pub fn loc(&mut self, ty: u8, pos: i16, text: bool) {
		match ty {
			0 => self.cursor[0] = pos,
			1 => self.cursor[1] = pos,
			2 => self.cursor[0] += pos,
			3 => self.cursor[1] += pos,
			_ => panic!(),
		}
		if text {
			self.fixcur();
		} else {
			self.limit_cursor();
		}
		self.eol = false;
	}

	pub fn send_area(&mut self, area: [i16; 4]) -> Result<()> {
		if area[2] <= area[0] || area[3] <= area[1] { return Ok(()) }
		self.writer.write(&[0])?;
		self.writer.write(&area[0].to_le_bytes())?;
		self.writer.write(&area[1].to_le_bytes())?;
		self.writer.write(&area[2].to_le_bytes())?;
		self.writer.write(&area[3].to_le_bytes())?;

		// respect to hist
		for y in area[1] as usize..area[3] as usize {
			for x in area[0] as usize..area[2] as usize {
				//         <-----> s = 7
				// scr:    xxxxxxx
				// hst: xxxxxxx<-- h = 3
				//        ^ y = 2, out
				let cell = if y < self.histcur {
					let yy = self.histcur - y - 1;
					self.history[yy][x]
				} else {
					let yy = y - self.histcur;
					self.buffer[yy][x]
				};

				// old simple get
				// let cell = self.buffer[y][x];

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

	pub fn damage_all(&mut self) {
		self.damage = [0, 0, self.size[0], self.size[1]];
	}

	pub fn include_damage(&mut self, mut damage: [i16; 4]) {
		if damage[2] <= damage[0] || damage[3] <= damage[1] { return }
		if damage[0] < 0 {
			damage[0] = 0;
		}
		if damage[1] < 0 {
			damage[1] = 0;
		}
		if damage[2] > self.size[0] {
			damage[2] = self.size[0];
		}
		if damage[3] > self.size[1] {
			damage[3] = self.size[1];
		}
		if self.damage[2] == 0 {
			self.damage = damage;
		}

		self.damage[0] = self.damage[0].min(damage[0]);
		self.damage[1] = self.damage[1].min(damage[1]);
		self.damage[2] = self.damage[2].max(damage[2]);
		self.damage[3] = self.damage[3].max(damage[3]);
	}

	pub fn send_damage(&mut self) -> Result<()> {
		// eprintln!("send dmg {:?}", self.damage);
		self.send_area(self.damage)?;
		self.send_cursor()?;
		self.writer.flush()?;
		self.damage = [0; 4];
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
