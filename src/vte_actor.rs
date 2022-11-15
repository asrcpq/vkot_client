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
				}
				1 => {
					// bold
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
						.rgb_from_256color(arg as u8 - 82)
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
		match action {
			'm' => {
				self.set_sgr(simple)?;
			}
			'A' => {
				self.wh.loc(3, -(simple[0].max(1) as i16));
			}
			'B' => {
				self.wh.loc(3, simple[0].max(1) as i16);
			}
			'C' => {
				self.wh.loc(2, simple[0].max(1) as i16);
			}
			'D' => {
				self.wh.loc(2, -(simple[0].max(1) as i16));
			}
			'K' => {
				let ty = simple.get(0).cloned().unwrap_or(0);
				self.wh.erase_display(ty, true).unwrap();
			}
			'J' => {
				let ty = simple.get(0).cloned().unwrap_or(0);
				self.wh.erase_line(ty, true).unwrap();
			}
			'H' | 'f' => {
				// coord start from 1
				let px = simple.get(0).cloned().unwrap_or(1) as i16;
				let py = simple.get(1).cloned().unwrap_or(1) as i16;
				self.wh.loc(0, py - 1);
				self.wh.loc(1, px - 1);
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

	fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
		match byte {
			b'M' => self.wh.scroll(false),
			_ => eprintln!("uh esc {:?}", byte as char),
		}
	}
}
