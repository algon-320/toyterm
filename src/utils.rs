#![allow(unused)]

pub mod io {
    use std::os::unix::io::{AsRawFd as _, OwnedFd};

    pub struct FdIo<'a>(pub &'a OwnedFd);

    impl<'a> std::io::Write for FdIo<'a> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let nb = nix::unistd::write(self.0.as_raw_fd(), bytes)?;
            Ok(nb)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> std::io::Read for FdIo<'a> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let nb = nix::unistd::read(self.0.as_raw_fd(), buf)?;
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

pub mod extension {
    pub trait GetMutPair<T> {
        fn get_mut_pair(&mut self, a: usize, b: usize) -> (&mut T, &mut T);
    }

    impl<T> GetMutPair<T> for [T] {
        fn get_mut_pair(&mut self, a: usize, b: usize) -> (&mut T, &mut T) {
            assert!(a != b && a < self.len() && b < self.len());

            use std::cmp::{max, min};
            let (a, b, swapped) = (min(a, b), max(a, b), a > b);

            // <--------xyz-------->
            // <--x--><-----yz----->
            //        <--y--><--z-->
            // .......a......b......
            let xyz = self;
            let (_x, yz) = xyz.split_at_mut(a);
            let (y, z) = yz.split_at_mut(b - a);
            let mut1 = &mut y[0];
            let mut2 = &mut z[0];

            if swapped {
                (mut2, mut1)
            } else {
                (mut1, mut2)
            }
        }
    }

    impl<T> GetMutPair<T> for std::collections::VecDeque<T> {
        fn get_mut_pair(&mut self, a: usize, b: usize) -> (&mut T, &mut T) {
            assert!(a != b && a < self.len() && b < self.len());

            let (fst, snd) = self.as_mut_slices();

            use std::cmp::{max, min};
            let (a, b, swapped) = (min(a, b), max(a, b), a > b);
            debug_assert!(a < b);

            let (mut1, mut2) = if b < fst.len() {
                debug_assert!(a < fst.len() && b < fst.len());
                let i = a;
                let j = b;
                fst.get_mut_pair(i, j)
            } else if fst.len() <= a {
                debug_assert!(fst.len() <= a && fst.len() <= b);
                let i = a - fst.len();
                let j = b - fst.len();
                snd.get_mut_pair(i, j)
            } else {
                debug_assert!(a < fst.len() && (b - fst.len()) < snd.len());
                let i = a;
                let j = b - fst.len();
                (&mut fst[i], &mut snd[j])
            };

            if swapped {
                (mut2, mut1)
            } else {
                (mut1, mut2)
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_slice() {
            let mut a = [0, 1, 2, 3, 4];
            let slice = a.as_mut_slice();

            let (x, y) = slice.get_mut_pair(0, 1);
            assert_eq!(*x, 0);
            assert_eq!(*y, 1);

            let (x, y) = slice.get_mut_pair(1, 0);
            assert_eq!(*x, 1);
            assert_eq!(*y, 0);

            let (x, y) = slice.get_mut_pair(0, 4);
            assert_eq!(*x, 0);
            assert_eq!(*y, 4);

            let (x, y) = slice.get_mut_pair(4, 0);
            assert_eq!(*x, 4);
            assert_eq!(*y, 0);
        }

        #[test]
        fn test_deque() {
            use std::collections::VecDeque;

            let mut deq = VecDeque::with_capacity(5);
            deq.push_back(2);
            deq.push_back(3);
            deq.push_back(4);
            deq.push_front(1);
            deq.push_front(0);

            assert_eq!(deq[0], 0);
            assert_eq!(deq[1], 1);
            assert_eq!(deq[2], 2);
            assert_eq!(deq[3], 3);
            assert_eq!(deq[4], 4);

            let (x, y) = deq.get_mut_pair(0, 1);
            assert_eq!(*x, 0);
            assert_eq!(*y, 1);
            let (x, y) = deq.get_mut_pair(1, 0);
            assert_eq!(*x, 1);
            assert_eq!(*y, 0);

            let (x, y) = deq.get_mut_pair(1, 2);
            assert_eq!(*x, 1);
            assert_eq!(*y, 2);
            let (x, y) = deq.get_mut_pair(2, 1);
            assert_eq!(*x, 2);
            assert_eq!(*y, 1);

            let (x, y) = deq.get_mut_pair(2, 3);
            assert_eq!(*x, 2);
            assert_eq!(*y, 3);
            let (x, y) = deq.get_mut_pair(3, 2);
            assert_eq!(*x, 3);
            assert_eq!(*y, 2);
        }
    }
}
