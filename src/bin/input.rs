use std::io::Result;

use vkot_client::client::Client;
use vkot_client::msg::ServerMsg;

fn main() -> Result<()> {
	let mut lines: Vec<Vec<char>> = vec![vec![]];
	let mut client = Client::default();
	loop {
		let event = client.poll_event().unwrap();
		if let ServerMsg::Getch(ch) = event {
			if ch == '\r' as u32 {
				lines.push(Vec::new());
			} else {
				lines.last_mut().unwrap().push(char::from_u32(ch).unwrap());
			}
		}
		client.clear()?;
		for (ln, line) in lines.iter().enumerate() {
			if ln % 2 == 0 {
				client.set_color([1.0, 0.0, 1.0, 1.0])?;
			} else {
				client.set_color([0.0, 1.0, 1.0, 1.0])?;
			}
			client.move_cursor([0, ln as u32])?;
			client.print(&line.iter().collect::<String>())?;
		}
		client.flush()?;
	}
}
