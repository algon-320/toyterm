pub fn pretty_format_ascii_bytes(bytes: &[u8]) -> String {
    const TABLE: [&str; 32] = [
        "NUL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "BEL", "BS", "HT", "LF", "VT", "FF", "CR",
        "SO", "SI", "DLE", "DC1", "DC2", "DC3", "DC4", "NAK", "SYN", "ETB", "CAN", "EM", "SUB",
        "ESC", "FS", "GS", "RS", "US",
    ];
    bytes
        .iter()
        .map(|c| {
            TABLE
                .get(*c as usize)
                .map(|s| format!("{{{}(x{:02X})}}", s, c))
                .unwrap_or_else(|| char::from(*c).to_string())
        })
        .fold("".to_string(), |s, x| s + &x)
}

pub fn wrap_range<T: Ord>(v: T, l: T, h: T) -> T {
    if v < l {
        l
    } else if h < v {
        h
    } else {
        v
    }
}
