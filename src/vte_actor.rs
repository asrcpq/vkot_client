use crate::color_table::ColorTable;
use crate::client::WriteHalf;

pub struct VteActor {
	pub wh: WriteHalf,
	color_table: ColorTable,
}

impl VteActor {
	pub fn new(wh: WriteHalf) -> Self {
		Self {
			wh,
			color_table: Default::default(),
		}
	}

	pub fn set_sgr(
		&mut self,
		simple: Vec<u16>,
	) -> std::io::Result<()> {
		let mut iter = simple.into_iter();
		loop {
			let arg = match iter.next() {
				Some(x) => x,
				None => return Ok(()),
			};
			match arg {
				0 => {
					self.wh.fg_color(u32::MAX);
					self.wh.bg_color(0);
					self.wh.reverse_color(false);
					self.wh.set_decoration(u32::MAX, 0);
				}
				1 => {
					// bold
				}
				4 => {
					self.wh.set_decoration(1 << 2, 1);
				}
				7 => {
					self.wh.reverse_color(true);
				}
				24 => {
					self.wh.set_decoration(1 << 2, 0);
				}
				27 => {
					self.wh.reverse_color(false);
				}
				30..=37 => {
					self.wh.fg_color(self
						.color_table
						.rgb_from_256color(arg as u8 - 30)
					);
				}
				38 => {
					let nx = iter.next().unwrap();
					if nx == 5 {
						let nx = iter.next().unwrap();
						self.wh.fg_color(self
							.color_table
							.rgb_from_256color(nx as u8)
						);
					} else {
						eprintln!("uh color");
					}
				}
				39 => {
					self.wh.fg_color(u32::MAX)
				}
				40..=47 => {
					self.wh.bg_color(self
						.color_table
						.rgb_from_256color(arg as u8 - 40)
					);
				}
				48 => {
					let nx = iter.next().unwrap();
					if nx == 5 {
						let nx = iter.next().unwrap();
						self.wh.bg_color(self
							.color_table
							.rgb_from_256color(nx as u8)
						);
					} else {
						eprintln!("uh color");
					}
				}
				49 => {
					self.wh.fg_color(0);
				}
				90..=97 => {
					self.wh.fg_color(self
						.color_table
						.rgb_from_256color(arg as u8 - 82)
					);
				}
				100..=107 => {
					self.wh.bg_color(self
						.color_table
						.rgb_from_256color(arg as u8 - 92)
					);
				}
				_ => {
					eprintln!("uh color {:?}", arg);
					return Ok(())
				}
			}
		}
	}

	pub fn csi_easy(
		&mut self,
		simple: Vec<u16>,
		interm: &[u8],
		action: char,
	) -> std::io::Result<()> {
		trait CsiVec {
			fn gv(&self, idx: usize) -> u16;
			fn gv0(&self, idx: usize) -> u16;
		}
		impl CsiVec for Vec<u16> {
			fn gv(&self, idx: usize) -> u16 {
				self.get(idx).cloned().unwrap_or(0).max(1)
			}
			fn gv0(&self, idx: usize) -> u16 {
				self.get(idx).cloned().unwrap_or(0)
			}
		}
		match action {
			'm' => {
				self.set_sgr(simple)?;
			}
			'A' => {
				self.wh.loc(3, -(simple.gv(0) as i16));
			}
			'B' => {
				self.wh.loc(3, simple.gv(0) as i16);
			}
			'C' => {
				self.wh.loc(2, simple.gv(0) as i16);
			}
			'D' => {
				self.wh.loc(2, -(simple.gv(0) as i16));
			}
			'J' => {
				let ty = simple.gv0(0);
				self.wh.erase_display(ty);
			}
			'K' => {
				let ty = simple.gv0(0);
				self.wh.erase_line(ty);
			}
			'H' | 'f' => {
				// coord start from 1
				let px = simple.gv(0);
				let py = simple.gv(1);
				self.wh.loc(0, py as i16 - 1);
				self.wh.loc(1, px as i16 - 1);
			}
			'h' | 'l' => {
				if simple.is_empty() {
					return Ok(())
				}
				if simple[0] == 2004 {
					// backet copy/paste
					return Ok(())
				}
				if simple[0] == 1 {
					// application mode
					return Ok(())
				}
				eprintln!(
					"uh csi {}: {:?} {}",
					action,
					simple,
					String::from_utf8_lossy(interm),
				)
			}
			'X' => {
				let count = simple.gv(0);
				self.wh.ech(count as i16);
			}
			_ => {
				eprintln!(
					"uh csi {}: {:?} {}",
					action,
					simple,
					String::from_utf8_lossy(interm),
				)
			}
		}
		Ok(())
	}
}

impl vte::Perform for VteActor {
	fn print(&mut self, c: char) {
		self.wh.put(c);
	}

	fn execute(&mut self, b: u8) {
		match b {
			b'\n' => {
				self.wh.newline();
				return
			}
			b'\x08' => {
				self.wh.loc(2, -1);
			}
			b'\x0d' => {
				self.wh.loc(0, 0);
			}
			b'\x09' => {
				self.wh.tab();
			}
			b'\x07' => {
				eprintln!("beep!")
			}
			0 => {}, // ignore
			b => eprintln!("uh c0: {}", b),
		}
	}

	fn csi_dispatch(
		&mut self,
		params: &vte::Params,
		interm: &[u8],
		_ignore: bool,
		action: char,
	) {
		let simple = params.iter().map(|x| x[0]).collect::<Vec<u16>>();
		self.csi_easy(simple, interm, action).unwrap();
	}

	fn esc_dispatch(&mut self, interm: &[u8], _ignore: bool, byte: u8) {
		match byte {
			b'M' => self.wh.scroll(false),
			b'=' => {} // ignore keypad
			b'>' => {} // ignore keypad
			_ => eprintln!("uh esc {:?} {:?}", byte as char, interm),
		}
	}
}
