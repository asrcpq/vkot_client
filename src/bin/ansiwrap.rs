use std::io::{Read, Write};
use std::process::{Command, Stdio, ChildStdout};
use std::sync::mpsc::{channel, Sender};

use vkot_client::client::{Client, ReadHalf, WriteHalf};
use vkot_client::msg::ServerMsg;
use vkot_client::color_table::ColorTable;

struct VteActor {
	wh: WriteHalf,
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

enum Msg {
	Vtc(ServerMsg),
	CmdRead(usize, [u8; 1024]),
	Exit,
}

fn vtc_thread(mut rh: ReadHalf, tx: Sender<Msg>) {
	loop {
		let server_msg = rh.poll_event().unwrap();
		tx.send(Msg::Vtc(server_msg)).unwrap();
	}
}

fn cmd_thread(mut stdout: ChildStdout, tx: Sender<Msg>) {
	loop {
		let mut buf = [0u8; 1024];
		let len = stdout.read(&mut buf).unwrap();
		if len == 0 {
			tx.send(Msg::Exit).unwrap();
			break
		}
		// eprintln!("get buf {:?}", String::from_utf8_lossy(&buf[0..len]));
		tx.send(Msg::CmdRead(len, buf)).unwrap();
	}
}

fn main() {
	let mut parser = vte::Parser::new();
	let (rh, mut wh) = Client::default().unwrap();
	wh.reset();
	let mut va = VteActor::new(wh);
	let args = std::env::args().collect::<Vec<String>>();
	let child = Command::new(&args[1])
		.args(&args[2..])
		.stdout(Stdio::piped())
		.stdin(Stdio::piped())
		.spawn()
		.unwrap();
	let (tx, rx) = channel();
	let tx2 = tx.clone();
	std::thread::spawn(move || vtc_thread(rh, tx));
	std::thread::spawn(move || cmd_thread(child.stdout.unwrap(), tx2));
	let mut child_stdin = child.stdin.unwrap();
	loop {
		match rx.recv().unwrap() {
			Msg::CmdRead(len, buf) => {
				eprintln!("len {}", len);
				for byte in buf[0..len].iter() {
					parser.advance(&mut va, *byte);
				}
			}
			Msg::Vtc(vtc) => {
				match vtc {
					ServerMsg::Getch(ch) => {
						if ch < 127 {
							eprintln!("{}", ch);
							child_stdin.write(&[ch as u8]).unwrap();
						}
					}
					_ => {},
				}
			},
			Msg::Exit => break,
		}
	}
}
