use nix::pty::Winsize;
use std::io::{Read, Write};
use std::os::unix::io::{RawFd, FromRawFd};
use std::sync::mpsc::{channel, Sender};

use crate::client::{Client, ReadHalf};
use crate::msg::ServerMsg;
use crate::vte_actor::VteActor;

nix::ioctl_write_ptr_bad!(tiocswinsz, nix::libc::TIOCSWINSZ, Winsize);

enum Msg {
	Vtc(ServerMsg),
	CmdRead(u8),
	Exit,
}

fn vtc_thread(mut rh: ReadHalf, tx: Sender<Msg>) {
	loop {
		let server_msg = rh.poll_event().unwrap();
		tx.send(Msg::Vtc(server_msg)).unwrap();
	}
}

#[allow(unused_imports)]
fn cmd_thread(fd: RawFd, tx: Sender<Msg>) {
	let file = unsafe {std::fs::File::from_raw_fd(fd)};
	for byte in file.bytes() {
		let byte = match byte {
			Ok(b) => b,
			Err(_) => {
				eprintln!("EOF");
				break
			}
		};
		tx.send(Msg::CmdRead(byte)).unwrap();
	}
	tx.send(Msg::Exit).unwrap();
}

fn size_conv(size: [i16; 2]) -> Winsize {
	Winsize {
		ws_row: size[0] as u16,
		ws_col: size[1] as u16,
		ws_xpixel: 0,
		ws_ypixel: 0,
	}
}

pub struct VteMaster {
	ws: Winsize,
	va: VteActor,
	rh: Option<ReadHalf>,
	master: RawFd,
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
		let tsize = if let ServerMsg::Resized(tsize) = event {
			tsize
		} else {
			panic!("First msg not size!");
		};

		wh.reset();
		let va = VteActor::new(wh);

		let mut result = Self {
			ws: size_conv([0, 0]), // useless
			va,
			rh: Some(rh),
			master,
		};
		result.resize(tsize);
		result
	}

	pub fn run(&mut self, master: RawFd) {
		let mut parser = vte::Parser::new();

		let (tx, rx) = channel();
		let tx2 = tx.clone();
		let rh = self.rh.take().unwrap();
		std::thread::spawn(move || vtc_thread(rh, tx));
		std::thread::spawn(move || cmd_thread(master, tx2));
		let mut file = unsafe {std::fs::File::from_raw_fd(master)};
		loop {
			match rx.recv().unwrap() {
				Msg::CmdRead(byte) => {
					parser.advance(&mut self.va, byte);
				}
				Msg::Vtc(vtc) => {
					match vtc {
						ServerMsg::Getch(ch) => {
							if ch < 127 {
								file.write(&[ch as u8]).unwrap();
							}
						}
						ServerMsg::Resized(new_size) => {
							self.resize(new_size);
						},
					}
				},
				Msg::Exit => break,
			}
		}
	}
}
