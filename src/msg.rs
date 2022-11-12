use anyhow::{anyhow, Result};
use std::convert::TryInto;

fn read_u32(bytes: &[u8]) -> u32 {
	u32::from_le_bytes(bytes.try_into().unwrap())
}

fn read_i16(bytes: &[u8]) -> i16 {
	i16::from_le_bytes(bytes.try_into().unwrap())
}

pub enum ServerMsg {
	Getch(u32),
	Resized([i16; 2]),
	Skey([u8; 2]),
}

impl ServerMsg {
	pub fn from_buf(buf: &[u8], offset: &mut usize) -> Result<Vec<Self>> {
		let mut result = Vec::new();
		while *offset < buf.len() {
			let b0 = buf[*offset];
			*offset += 1;
			let msg = match b0 {
				0 => {
					let ch = read_u32(&buf[*offset..*offset + 4]);
					*offset += 4;
					Self::Getch(ch)
				}
				1 => {
					let u1 = read_i16(&buf[*offset..*offset + 2]);
					let u2 = read_i16(&buf[*offset + 2..*offset + 4]);
					*offset += 4;
					Self::Resized([u1, u2])
				}
				2 => {
					let b1 = buf[*offset];
					let b2 = buf[*offset + 1];
					*offset += 2;
					Self::Skey([b1, b2])
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
