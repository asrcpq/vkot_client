fn main() {
	let (tx, _rx) = std::sync::mpsc::channel();
	vkot_client::apaterm::start(tx, vec![std::ffi::CString::new("sh").unwrap()]);
}
