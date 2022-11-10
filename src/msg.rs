use anyhow::{anyhow, Result};
use byteorder::{ByteOrder, LittleEndian as Ble};

pub enum ServerMsg {
	Getch(u32),
	Resized([u32; 2])
}

impl ServerMsg {
	pub fn from_buf(buf: &[u8], offset: &mut usize) -> Result<Vec<Self>> {
		let mut result = Vec::new();
		while *offset < buf.len() {
			let b0 = buf[*offset];
			*offset += 1;
			let msg = match b0 {
				b'g' => {
					let ch = Ble::read_u32(&buf[*offset..*offset + 4]);
					*offset += 4;
					Self::Getch(ch)
				}
				b'r' => {
					let u1 = Ble::read_u32(&buf[*offset..*offset + 4]);
					let u2 = Ble::read_u32(&buf[*offset + 4..*offset + 8]);
					*offset += 8;
					Self::Resized([u1, u2])
				}
				c => return Err(anyhow!("unknown message type {:?}", c as char))
			};
			result.push(msg);
		}
		if *offset != buf.len() {
			eprintln!("bad msg: {:?}", String::from_utf8_lossy(buf))
		}
		Ok(result)
	}
}
