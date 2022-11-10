use std::io::Read;
use std::process::{Command, Stdio, ChildStdout};
use std::sync::mpsc::{channel, Sender};

use vkot_client::client::{Client, ReadHalf};
use vkot_client::msg::ServerMsg;

#[derive(Default)]
struct VteActor;
impl vte::Perform for VteActor {
	fn print(&mut self, c: char) {
		eprintln!("print {}", c);
	}
}

enum Msg {
	Vtc(ServerMsg),
	CmdRead(usize, [u8; 1024]),
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
		if len == 0 { break }
		eprintln!("get buf {:?}", String::from_utf8_lossy(&buf[0..len]));
		tx.send(Msg::CmdRead(len, buf)).unwrap();
	}
}

fn main() {
	let mut va = VteActor::default();
	let mut parser = vte::Parser::new();
	let (rh, wh) = Client::default().unwrap();
	let args = std::env::args().collect::<Vec<String>>();
	let mut child = Command::new(&args[1])
		.stdout(Stdio::piped())
		.stdin(Stdio::piped())
		.spawn()
		.unwrap();
	let (tx, rx) = channel();
	let tx2 = tx.clone();
	std::thread::spawn(move || vtc_thread(rh, tx));
	std::thread::spawn(move || cmd_thread(child.stdout.unwrap(), tx2));
	loop {
		match rx.recv().unwrap() {
			Msg::CmdRead(len, buf) => {
				eprintln!("len {}", len);
				for byte in buf[0..len].iter() {
					parser.advance(&mut va, *byte);
				}
			}
			_ => {},
		}
	}
}
