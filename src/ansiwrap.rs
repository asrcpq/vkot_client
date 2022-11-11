use std::io::{Read, Write};
use std::os::unix::io::{RawFd, FromRawFd};
use std::sync::mpsc::{channel, Sender};

use crate::client::{Client, ReadHalf};
use crate::msg::ServerMsg;
use crate::vte_actor::VteActor;

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

#[derive(Default)]
pub struct VteMaster {}

impl VteMaster {
	pub fn run(&self, master: RawFd) {
		let mut parser = vte::Parser::new();
		let (mut rh, mut wh) = Client::default().unwrap();
		let event = rh.poll_event().unwrap();
		let tsize = if let ServerMsg::Resized(tsize) = event {
			tsize
		} else {
			panic!("First msg not size!");
		};
		wh.resize(tsize);
		wh.reset();
		let mut va = VteActor::new(wh);
		let (tx, rx) = channel();
		let tx2 = tx.clone();
		std::thread::spawn(move || vtc_thread(rh, tx));
		std::thread::spawn(move || cmd_thread(master, tx2));
		let mut file = unsafe {std::fs::File::from_raw_fd(master)};
		loop {
			match rx.recv().unwrap() {
				Msg::CmdRead(byte) => {
					parser.advance(&mut va, byte);
				}
				Msg::Vtc(vtc) => {
					match vtc {
						ServerMsg::Getch(ch) => {
							if ch < 127 {
								eprintln!("{}", ch);
								file.write(&[ch as u8]).unwrap();
							}
						}
						ServerMsg::Resized(new_size) => {
							va.wh.resize(new_size);
						},
					}
				},
				Msg::Exit => break,
			}
		}
	}
}
