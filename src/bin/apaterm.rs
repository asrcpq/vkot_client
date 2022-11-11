use nix::fcntl::{open, OFlag};
use nix::pty::{grantpt, posix_openpt, ptsname, unlockpt};
use nix::sys::stat::Mode;
use nix::unistd;
use vkot_client::ansiwrap::VteMaster;

use std::os::unix::io::IntoRawFd;
use std::path::Path;

fn start() {
	let master_fd = posix_openpt(OFlag::O_RDWR).unwrap();
	grantpt(&master_fd).unwrap();
	unlockpt(&master_fd).unwrap();
	let slave_name = unsafe { ptsname(&master_fd).unwrap() };
	let slave_fd = open(Path::new(&slave_name), OFlag::O_RDWR, Mode::empty()).unwrap();
	let master_fd = master_fd.into_raw_fd();
	let mut console = VteMaster::new(master_fd);

	match unsafe {unistd::fork()} {
		Ok(unistd::ForkResult::Parent { child: _, .. }) => {
			unistd::close(slave_fd).unwrap();
			console.run(master_fd)
		}
		Ok(unistd::ForkResult::Child) => {
			unistd::close(master_fd).unwrap();

			// create process group
			unistd::setsid().unwrap();

			nix::ioctl_write_int_bad!(tiocsctty, libc::TIOCSCTTY);
			unsafe { tiocsctty(slave_fd, 0).unwrap() };

			unistd::dup2(slave_fd, 0).unwrap(); // stdin
			unistd::dup2(slave_fd, 1).unwrap(); // stdout
			unistd::dup2(slave_fd, 2).unwrap(); // stderr
			unistd::close(slave_fd).unwrap();

			use std::ffi::CString;
			let args = std::env::args().collect::<Vec<_>>();
			std::env::set_var("TERM", "vt100");

			let path = CString::new(args[1].clone()).unwrap();
			let args = args[1..].iter()
				.cloned()
				.map(|x| CString::new(x).unwrap())
				.collect::<Vec<_>>();
			unistd::execvp(&path, &args).unwrap();
		}
		Err(_) => panic!(),
	}
}

fn main() {
	start();
}
