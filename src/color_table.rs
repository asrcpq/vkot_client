pub struct ColorTable {
	data: Vec<[f32; 4]>,
}

// big endian
fn byte2hex(b1: u8, b2: u8) -> u8 {
	let mut result = if b2 > b'a' {
		b2 - b'a' + 10
	} else {
		b2
	};
	result += if b1 > b'a' {
		b1 - b'a' + 10
	} else {
		b1
	} * 16;
	result
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
			data.push([
				r as f32 / 255.0,
				g as f32 / 255.0,
				b as f32 / 255.0,
				1.0,
			]);
		}
		Self {data}
	}
}

impl ColorTable {
	pub fn rgb_from_256color(&self, color: u8) -> [f32; 4] {
		self.data[color as usize]
	}
}
