#![allow(unused)]

pub mod fd {
    use std::fs::File;
    use std::os::unix::io::{FromRawFd, RawFd};

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct OwnedFd(RawFd);

    impl OwnedFd {
        /// Safety: `fd` must not be used in other place
        pub unsafe fn from_raw_fd(fd: RawFd) -> Self {
            Self(fd)
        }

        pub fn as_raw(&self) -> RawFd {
            self.0
        }

        pub fn dup(&self) -> std::io::Result<Self> {
            let new_fd = nix::unistd::dup(self.0)?;
            Ok(OwnedFd(new_fd))
        }

        pub fn into_file(self) -> File {
            let raw_fd = self.0;
            let file = unsafe { File::from_raw_fd(raw_fd) };
            std::mem::forget(self);
            file
        }
    }

    impl Drop for OwnedFd {
        fn drop(&mut self) {
            let _ = nix::unistd::close(self.0);
        }
    }

    impl std::io::Write for OwnedFd {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let nb = nix::unistd::write(self.as_raw(), bytes)?;
            Ok(nb)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl std::io::Read for OwnedFd {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let nb = nix::unistd::read(self.as_raw(), buf)?;
            Ok(nb)
        }
    }
}

pub mod utf8 {
    pub fn process_utf8<'b, F>(buf: &'b [u8], mut callback: F) -> &[u8]
    where
        F: FnMut(Result<&'b str, &'b [u8]>),
    {
        let mut i = 0;
        loop {
            match std::str::from_utf8(&buf[i..]) {
                Ok(utf8) => {
                    callback(Ok(utf8));
                    return &[];
                }

                Err(err) => {
                    let j = i + err.valid_up_to();
                    let utf8 = unsafe { std::str::from_utf8_unchecked(&buf[i..j]) };
                    callback(Ok(utf8));
                    i = j;

                    match err.error_len() {
                        Some(next) => {
                            callback(Err(&buf[i..(i + next)]));
                            i += next;
                        }
                        None => {
                            return &buf[i..];
                        }
                    }
                }
            }
        }
    }

    pub fn process_utf8_lossy<F>(buf: &[u8], mut callback: F) -> &[u8]
    where
        F: FnMut(&str),
    {
        process_utf8(buf, |res| {
            if let Ok(utf8) = res {
                callback(utf8)
            }
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_process_utf8_lossy() {
            {
                let mut buf = b"abc".to_vec();
                let mut res = String::new();
                let rem = process_utf8_lossy(&mut buf[..], |s| res.push_str(s));
                assert_eq!(rem.len(), 0);
                assert_eq!(&res, "abc");
            }

            {
                let mut buf = b"\xE3\x81\x82\xE3\x81\x84\xE3\x81\x86".to_vec();
                let mut res = String::new();
                let rem = process_utf8_lossy(&mut buf[..], |s| res.push_str(s));
                assert_eq!(rem.len(), 0);
                assert_eq!(&res, "あいう");
            }

            {
                let mut buf = b"\xE3\x81\x82\xE3\x81\x84\xE3".to_vec();
                let buf_len = buf.len();
                let mut res = String::new();

                let rem = process_utf8_lossy(&mut buf[..], |s| res.push_str(s));
                assert_eq!(rem.len(), 1);
                assert_eq!(rem[0], b'\xE3');

                let rem_offset = buf_len - rem.len();
                buf.copy_within(rem_offset.., 0);

                buf[1..3].copy_from_slice(b"\x81\x86");
                let rem = process_utf8_lossy(&mut buf[..3], |s| res.push_str(s));
                assert_eq!(rem.len(), 0);

                assert_eq!(&res, "あいう");
            }

            {
                let mut buf = b"\xE3\x81\x82\xE3\x81\x84\xE3\x81".to_vec();
                let buf_len = buf.len();
                let mut res = String::new();

                let rem = process_utf8_lossy(&mut buf[..], |s| res.push_str(s));
                assert_eq!(rem.len(), 2);
                assert_eq!(&rem[0..2], b"\xE3\x81");

                let rem_offset = buf_len - rem.len();
                buf.copy_within(rem_offset.., 0);

                buf[2..3].copy_from_slice(b"\x86");
                let rem = process_utf8_lossy(&mut buf[..3], |s| res.push_str(s));
                assert_eq!(rem.len(), 0);

                assert_eq!(&res, "あいう");
            }
        }
    }
}
