use std::io::Read;
use std::process::{Command, Stdio, ChildStdout};
use std::sync::mpsc::{channel, Sender};

use vkot_client::client::{Client, ReadHalf, WriteHalf};
use vkot_client::msg::ServerMsg;

struct VteActor {
	wh: WriteHalf,
}

impl VteActor {
	pub fn new(wh: WriteHalf) -> Self {
		Self {
			wh
		}
	}
}

impl vte::Perform for VteActor {
	fn print(&mut self, c: char) {
		self.wh.print(c).unwrap();
		self.wh.flush().unwrap();
	}

	fn execute(&mut self, b: u8) {
		if b == b'\n' {
			self.wh.loc(3, 1).unwrap();
			self.wh.loc(0, 0).unwrap();
			return
		}
	}

	fn csi_dispatch(
		&mut self,
		_params: &vte::Params,
		_intermediates: &[u8],
		_ignore: bool,
		action: char,
	) {
		eprintln!("csi {}", action)
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
		// eprintln!("get buf {:?}", String::from_utf8_lossy(&buf[0..len]));
		tx.send(Msg::CmdRead(len, buf)).unwrap();
	}
}

fn main() {
	let mut parser = vte::Parser::new();
	let (rh, wh) = Client::default().unwrap();
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
