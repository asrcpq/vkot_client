pub struct ColorTable {
	data: Vec<u32>,
}

fn byte2num(b: u8) -> u32 {
	let result = if (b'a'..=b'z').contains(&b) {
		b - b'a' + 10
	} else if (b'A'..=b'Z').contains(&b) {
		b - b'A' + 10
	} else if (b'0'..=b'9').contains(&b) {
		b - b'0'
	} else {
		panic!()
	};
	result as u32
}

// big endian
fn byte2hex(b1: u8, b2: u8) -> u32 {
	byte2num(b2) + byte2num(b1) * 16
}

impl Default for ColorTable {
	fn default() -> Self {
		let data_str = include_str!("color_table.txt");
		let mut data = Vec::new();
		for line in data_str.split('\n') {
			let line = line.trim_end();
			if line.is_empty() {continue}
			let bytes: Vec<u8> = line.bytes().collect();
			let r = byte2hex(bytes[0], bytes[1]);
			let g = byte2hex(bytes[2], bytes[3]);
			let b = byte2hex(bytes[4], bytes[5]);
			let c = (r << 24) + (g << 16) + (b << 8) + 255;
			data.push(c);
		}
		data[0] = 0x303030FF;
		data[4] = 0x3030C0FF;
		data[7] = 0xA0A0A0FF;
		data[8] = 0x707070FF;
		data[15] = 0xE0E0E0FF;
		Self {data}
	}
}

impl ColorTable {
	pub fn rgb_from_256color(&self, color: u8) -> u32 {
		self.data[color as usize]
	}
}
