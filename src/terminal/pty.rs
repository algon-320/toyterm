use nix::fcntl::{open, OFlag};
use nix::pty::{grantpt, posix_openpt, ptsname, unlockpt};
use nix::sys::stat::Mode;

use std::os::unix::io::RawFd;
use std::path::Path;

use crate::utils::*;

#[derive(Debug, Clone, Copy)]
pub struct PTY {
    pub master: RawFd,
    pub slave: RawFd,
}

impl PTY {
    pub fn open() -> Result<Self, String> {
        // Open a new PTY master
        let master_fd = err_str(posix_openpt(OFlag::O_RDWR))?;

        // Allow a slave to be generated for it
        err_str(grantpt(&master_fd))?;
        err_str(unlockpt(&master_fd))?;

        // Get the name of the slave
        let slave_name = err_str(unsafe { ptsname(&master_fd) })?;

        // Try to open the slave
        let slave_fd = err_str(open(Path::new(&slave_name), OFlag::O_RDWR, Mode::empty()))?;

        use std::os::unix::io::IntoRawFd;
        Ok(PTY {
            master: master_fd.into_raw_fd(),
            slave: slave_fd.into(),
        })
    }
}
