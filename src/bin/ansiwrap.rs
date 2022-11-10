use std::process::{Command, Stdio};
use vkot_client::client::Client;
use vkot_client::msg::ServerMsg;

struct VteActor;
impl vte::Perform for VteActor {}

fn main() {
	let args = std::env::args().collect::<Vec<String>>();
	let mut child = Command::new(&args[1])
		.stdout(Stdio::piped())
		.stdin(Stdio::piped())
		.spawn()
		.unwrap();
	child.wait().unwrap();
}
