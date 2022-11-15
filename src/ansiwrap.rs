use nix::pty::Winsize;
use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::io::{RawFd, FromRawFd};
use std::sync::mpsc::{channel, Sender, Receiver};
use vte::Parser;

use crate::client::{Client, ReadHalf};
use crate::msg::ServerMsg;
use crate::vte_actor::VteActor;
use skey::{Skey, SkType};
use skey::modtrack::ModifierTracker;

nix::ioctl_write_ptr_bad!(tiocswinsz, nix::libc::TIOCSWINSZ, Winsize);

enum Msg {
	Vtc(ServerMsg),
	CmdRead(Vec<u8>),
	Exit,
}

fn vtc_thread(mut rh: ReadHalf, tx: Sender<Msg>) {
	loop {
		let server_msg = rh.poll_event().unwrap();
		tx.send(Msg::Vtc(server_msg)).unwrap();
	}
}

pub struct Debugger{
	file: std::fs::File,
	pub state: i8,
	pub sleep: u64,
}

impl Debugger {
	pub fn new(sleep: u64) -> Self {
		let file = std::fs::File::create("/tmp/vkot_debug.txt").unwrap();
		Self {
			file,
			sleep,
			state: 0,
		}
	}

	pub fn p(&mut self, byte: u8) {
		self.file.write(&[byte]).unwrap();
		let ch = byte as char;
		if self.state == 1 {
			eprint!("[48;5;22m");
		} else {
			eprint!("[48;5;18m");
		}
		if ch.is_ascii_control() {
			eprint!("\\{}", byte);
		} else {
			eprint!("{}", ch);
		}
		eprint!("[0m");
		self.state = 1 - self.state;
	}
}

#[allow(unused_imports)]
fn cmd_thread(fd: RawFd, tx: Sender<Msg>) {
	let mut file = unsafe {std::fs::File::from_raw_fd(fd)};
	loop {
		let mut buf = vec![0u8; 1024];
		let len = match file.read(&mut buf) {
			Ok(len) => if len == 0 {
				eprintln!("Cmd eof");
				break
			} else {
				len
			}
			Err(_) => {
				eprintln!("Cmd unreadable");
				break
			}
		};
		buf.truncate(len);
		tx.send(Msg::CmdRead(buf)).unwrap();
	}
	tx.send(Msg::Exit).unwrap();
}

fn size_conv(size: [i16; 2]) -> Winsize {
	Winsize {
		ws_row: size[1] as u16,
		ws_col: size[0] as u16,
		ws_xpixel: 0,
		ws_ypixel: 0,
	}
}

pub struct VteMaster {
	ws: Winsize,
	va: VteActor,
	rh: Option<ReadHalf>,
	master: RawFd,
	parser: Parser,
	modtrack: ModifierTracker,
	debugger: Option<Debugger>,
	alt_on: bool,
}

impl VteMaster {
	fn resize(&mut self, tsize: [i16; 2]) {
		self.va.wh.resize(tsize);
		self.ws = size_conv(tsize);
		unsafe {tiocswinsz(self.master, &self.ws).unwrap(); }
	}

	pub fn new(master: RawFd) -> Self {
		let (mut rh, mut wh) = Client::default().unwrap();
		let event = rh.poll_event().unwrap();
		let parser = vte::Parser::new();
		let tsize = if let ServerMsg::Resized(tsize) = event {
			tsize
		} else {
			panic!("First msg not size!");
		};

		wh.reset();
		let va = VteActor::new(wh);

		let debugger = match std::env::var("VKOT_SYNC_DEBUG") {
			Ok(x) => Some(Debugger::new(x.parse::<u64>().unwrap())),
			_ => None,
		};

		let mut result = Self {
			ws: size_conv([0, 0]), // useless
			va,
			rh: Some(rh),
			master,
			parser,
			debugger,
			modtrack: Default::default(),
			alt_on: false,
		};
		result.resize(tsize);
		result
	}

	// return true = exit
	fn proc_msg(&mut self, file: &mut File, msg: Msg) -> bool {
		match msg {
			Msg::CmdRead(bytes) => {
				for byte in bytes.into_iter() {
					// if byte > 0 {eprint!("{:?}", byte as char);}
					self.parser.advance(&mut self.va, byte);
					if let Some(debugger) = self.debugger.as_mut() {
						debugger.p(byte);
						std::thread::sleep(std::time::Duration::from_millis(debugger.sleep));
						self.va.wh.send_damage().unwrap();
					}
				}
			}
			Msg::Vtc(vtc) => {
				match vtc {
					ServerMsg::Getch(ch) => {
						if ch < 127 {
							if self.alt_on {
								file.write(b"\x1b").unwrap();
								self.alt_on = false;
							}
							file.write(&[ch as u8]).unwrap();
						}
					}
					ServerMsg::Resized(new_size) => {
						self.resize(new_size);
					},
					ServerMsg::Skey(bytes) => {
						let skey = if let Some(skey) = Skey::des(bytes) {
							skey
						} else {
							return false
						};
						if !skey.down {
							match skey.ty {
								SkType::Modifier(3) => self.alt_on = false,
								_ => {},
							}
							return false
						}
						match skey.ty {
							SkType::Direction(x) => {
								match x {
									0 => {
										file.write(b"\x1b[D").unwrap();
									}
									1 => {
										file.write(b"\x1b[A").unwrap();
									}
									2 => {
										file.write(b"\x1b[C").unwrap();
									}
									3 => {
										file.write(b"\x1b[B").unwrap();
									}
									6 | 7 => {
										if self.modtrack.shift {
											let down = x == 7;
											self.va.wh.scroll_history_page(down);
										}
									}
									_ => {},
								}
							}
							SkType::Modifier(x) => {
								if x == 3 {
									self.alt_on = true;
								} else {
									self.modtrack.update_skey(skey);
								}
							}
							_ => {},
						}
					},
				}
			},
			Msg::Exit => return true,
		}
		false
	}

	#[allow(dead_code)]
	fn run_nolockframe(&mut self, mut file: File, rx: Receiver<Msg>) {
		loop {
			let msg = rx.recv().unwrap();
			if self.proc_msg(&mut file, msg) {
				return
			}
			self.va.wh.send_damage().unwrap();
		}
	}

	// TODO: only react(sending rendering info) at drawing msg
	fn run_lockframe(&mut self, mut file: File, rx: Receiver<Msg>) {
		enum RecvType {
			Block,
			Nonblock,
			Timeout(u64),
		}
		const FTIME: u64 = 10;

		let mut tryr = RecvType::Nonblock;
		let mut prev_send = std::time::SystemTime::now();
		// the send counter is designed to force refresh for like every 10ms
		let mut send_counter = 0;
		loop {
			let result = match tryr {
				RecvType::Block => rx.recv().ok(),
				RecvType::Nonblock => rx.try_recv().ok(),
				RecvType::Timeout(t) => {
					let slpt = std::time::Duration::from_millis(t);
					rx.recv_timeout(slpt).ok()
				}
			};
			match result {
				Some(msg) => {
					if self.proc_msg(&mut file, msg) {
						return
					}
					if send_counter >= 100 {
						send_counter = 0;
						let new_time = std::time::SystemTime::now();
						let dur = new_time.duration_since(prev_send)
							.unwrap()
							.as_millis() as u64;
						if dur > FTIME {
							prev_send = new_time;
							self.va.wh.send_damage().unwrap();
						}
					} else {
						send_counter += 1;
					}
					tryr = RecvType::Nonblock;
				}
				None => {
					let new_time = std::time::SystemTime::now();
					let dur = new_time.duration_since(prev_send)
						.unwrap()
						.as_millis() as u64;
					if dur > FTIME {
						prev_send = new_time;
						self.va.wh.send_damage().unwrap();
						tryr = RecvType::Block;
					} else {
						tryr = RecvType::Timeout(FTIME - dur + 1);
					}
				}
			}
		}
	}

	pub fn run(&mut self, master: RawFd) {
		let (tx, rx) = channel();
		let tx2 = tx.clone();
		let rh = self.rh.take().unwrap();
		std::thread::spawn(move || vtc_thread(rh, tx));
		std::thread::spawn(move || cmd_thread(master, tx2));
		let file = unsafe {File::from_raw_fd(master)};

		// self.run_nolockframe(file, rx);
		self.run_lockframe(file, rx);
	}
}
