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

	pub fn csi_easy(
		&mut self,
		simple: Vec<u16>,
		interm: &[u8],
		action: char,
	) -> std::io::Result<()> {
		match action {
			'm' => {
				let mut boffset = 0;
				let mut iter = simple.into_iter();
				loop {
					let arg = match iter.next() {
						Some(x) => x,
						None => break,
					};
					if arg == 1 {
						boffset = 8;
					} else if arg == 0 {
						boffset = 0;
						self.wh.set_color(u32::MAX);
					} else if (30..=37).contains(&arg) {
						self.wh.set_color(self
							.color_table
							.rgb_from_256color(arg as u8 - 30 + boffset)
						);
					} else if arg == 38 {
						let nx = iter.next().unwrap();
						if nx == 5 {
							let nx = iter.next().unwrap();
							self.wh.set_color(self
								.color_table
								.rgb_from_256color(nx as u8)
							);
						} else {
							unimplemented!();
						}
					} else {
						self.wh.set_color(u32::MAX);
					}
				}
			}
			'A' => {
				self.wh.loc(3, -(simple[0] as i16), false);
			}
			'B' => {
				self.wh.loc(3, simple[0] as i16, false);
			}
			'C' => {
				self.wh.loc(2, simple[0] as i16, false);
			}
			'D' => {
				self.wh.loc(2, -(simple[0] as i16), false);
			}
			'K' => {
				self.wh.erase_display(simple.get(0).cloned().unwrap_or(0));
				self.wh.send_damage().unwrap();
			}
			'J' => {
				self.wh.erase_line(simple.get(0).cloned().unwrap_or(0));
				self.wh.send_damage().unwrap();
			}
			'H' | 'f' => {
				let px = simple.get(0).cloned().unwrap_or(0) as i16;
				let py = simple.get(1).cloned().unwrap_or(0) as i16;
				self.wh.loc(0, py, false);
				self.wh.loc(1, px, false);
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
					"unknown csi {}: {:?} {}",
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
		self.wh.put(c, true).unwrap();
	}

	fn execute(&mut self, b: u8) {
		match b {
			b'\n' => {
				self.wh.newline();
				return
			}
			b'\x08' => {
				self.wh.loc(2, -1, true);
			}
			b'\x0d' => {
				self.wh.loc(0, 0, false);
			}
			b'\x09' => {
				self.wh.tab();
				self.wh.send_damage().unwrap();
			}
			b'\x07' => {
				eprintln!("beep!")
			}
			0 => {}, // ignore
			b => eprintln!("control char received: {}", b),
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
}
