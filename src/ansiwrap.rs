use std::io::{Read, Write};
use std::process::{Command, Stdio, ChildStdout};
use std::sync::mpsc::{channel, Sender};

use crate::client::{Client, ReadHalf};
use crate::msg::ServerMsg;
use crate::vte_actor::VteActor;

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

pub struct VteMaster {}

impl VteMaster {
	pub fn run() {
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
		let args = std::env::args().collect::<Vec<String>>();
		let child = Command::new(&args[1])
			.args(&args[2..])
			.env("COLUMNS", format!("{}", tsize[0]))
			.env("LINES", format!("{}", tsize[1]))
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
