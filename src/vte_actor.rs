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

	pub fn csi_easy(&mut self, simple: Vec<u16>, action: char) -> std::io::Result<()> {
		match action {
			'm' => {
				let mut boffset = 0;
				for arg in simple.into_iter() {
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
					}
				}
			}
			'A' => {
				self.wh.loc(3, -(simple[0] as i16));
			}
			'B' => {
				self.wh.loc(3, simple[0] as i16);
			}
			'C' => {
				self.wh.loc(2, simple[0] as i16);
			}
			'D' => {
				self.wh.loc(2, -(simple[0] as i16));
			}
			'K' => {
				self.wh.erase_display(simple.get(0).cloned().unwrap_or(0));
				self.wh.flush().unwrap();
			}
			'J' => {
				self.wh.erase_line(simple.get(0).cloned().unwrap_or(0));
				self.wh.flush().unwrap();
			}
			_ => {
				eprintln!("unknown csi {}", action)
			}
		}
		Ok(())
	}
}

impl vte::Perform for VteActor {
	fn print(&mut self, c: char) {
		self.wh.put(c).unwrap();
	}

	fn execute(&mut self, b: u8) {
		if b == b'\n' {
			self.wh.loc(3, 1);
			self.wh.loc(0, 0);
			return
		}
	}

	fn csi_dispatch(
		&mut self,
		params: &vte::Params,
		_intermediates: &[u8],
		_ignore: bool,
		action: char,
	) {
		let simple = params.iter().map(|x| x[0]).collect::<Vec<u16>>();
		self.csi_easy(simple, action).unwrap();
	}
}