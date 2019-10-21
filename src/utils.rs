pub fn err_str<T, E: ToString>(e: Result<T, E>) -> std::result::Result<T, String> {
    e.map_err(|e| e.to_string())
}

pub fn pretty_format_ascii_bytes(bytes: &[u8]) -> Vec<String> {
    const TABLE: [&str; 33] = [
        "NUL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "BEL", "BS", "HT", "LF", "VT", "FF", "CR",
        "SO", "SI", "DLE", "DC1", "DC2", "DC3", "DC4", "NAK", "SYN", "ETB", "CAN", "EM", "SUB",
        "ESC", "FS", "GS", "RS", "US", "` `",
    ];
    bytes
        .iter()
        .map(|c| {
            TABLE
                .get(*c as usize)
                .map(|s| format!("{}(x{:02X})", s, c))
                .unwrap_or_else(|| char::from(*c).to_string())
        })
        .collect()
}

// example: assert_eq!(parse_int_from_ascii(b"12345"), Some(12345))
pub fn parse_int_from_ascii(bytes: &[u8]) -> Option<u32> {
    if bytes.len() == 0 {
        return None;
    }
    let mut ret = 0;
    for c in bytes.iter() {
        ret *= 10;
        if char::from(*c).is_digit(10) {
            ret += (*c - b'0') as u32
        } else {
            return None;
        }
    }
    Some(ret)
}

pub fn setenv(name: &str, value: &str, overwrite: bool) -> Result<(), String> {
    let n = err_str(std::ffi::CString::new(name))?;
    let v = err_str(std::ffi::CString::new(value))?;
    let r = unsafe { nix::libc::setenv(n.as_ptr(), v.as_ptr(), if overwrite { 1 } else { 0 }) };
    if r == 0 {
        Ok(())
    } else {
        let en = nix::errno::from_i32(nix::errno::errno());
        Err(nix::Error::from_errno(en).to_string())
    }
}

pub fn fill_slice<T: Copy>(buf: &mut [T], v: T) {
    for x in buf {
        *x = v;
    }
}
