use std::marker::PhantomData;

use crate::utils::fd::OwnedFd;
use crate::utils::wrapper::pipe;

#[derive(Debug)]
pub struct Sender<T> {
    tx: OwnedFd,
    _phantom: PhantomData<*mut T>,
}

impl<T> Sender<T> {
    pub fn send(&mut self, val: T) {
        let size = std::mem::size_of::<T>();
        debug_assert!(0 < size);

        let ptr = &val as *const T as *const u8;
        std::mem::forget(val);

        let bytes: &[u8] = unsafe { std::slice::from_raw_parts(ptr, size) };

        use std::io::Write as _;
        self.tx.write_all(bytes).unwrap();
    }
}

unsafe impl<T: Send> Send for Sender<T> {}

#[derive(Debug)]
pub struct Receiver<T> {
    rx: OwnedFd,
    buf: Vec<u8>,
    _phantom: std::marker::PhantomData<*mut T>,
}

unsafe impl<T: Send> Send for Receiver<T> {}

impl<T> Receiver<T> {
    pub fn get_fd(&self) -> std::os::unix::io::RawFd {
        self.rx.as_raw()
    }

    pub fn recv(&mut self) -> T {
        let size = std::mem::size_of::<T>();
        debug_assert!(0 < size && size <= self.buf.len());

        use std::io::Read as _;
        self.rx.read_exact(&mut self.buf[..size]).unwrap();

        use std::mem::MaybeUninit;
        let mut maybe_uninit: MaybeUninit<T> = MaybeUninit::uninit();

        let val: T = {
            let src = self.buf.as_mut_ptr();
            let dst = maybe_uninit.as_mut_ptr() as *mut u8;
            unsafe { std::ptr::copy_nonoverlapping(src, dst, size) };
            unsafe { maybe_uninit.assume_init() }
        };

        val
    }
}

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let (rx, tx) = pipe().expect("pipe");

    let sender = Sender {
        tx,
        _phantom: PhantomData,
    };
    let receiver = Receiver {
        rx,
        buf: vec![0_u8; std::mem::size_of::<T>()],
        _phantom: PhantomData,
    };

    (sender, receiver)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel() {
        let (mut tx, mut rx) = channel::<i32>();

        tx.send(123_i32);
        let val = rx.recv();
        assert_eq!(val, 123_i32);

        tx.send(i32::MAX);
        let val = rx.recv();
        assert_eq!(val, i32::MAX);

        tx.send(i32::MIN);
        let val = rx.recv();
        assert_eq!(val, i32::MIN);

        let (mut tx, mut rx) = channel::<String>();
        tx.send("Hello".to_owned());
        let val = rx.recv();
        assert_eq!(val, "Hello".to_owned());

        let rc = std::rc::Rc::new(());
        let (mut tx, mut rx) = channel::<std::rc::Rc<()>>();
        tx.send(rc.clone());
        let val = rx.recv();
        drop(val);
        assert_eq!(std::rc::Rc::strong_count(&rc), 1);
    }
}
