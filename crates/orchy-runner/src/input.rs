pub fn is_mouse_sgr_prefix(b: &[u8]) -> bool {
    b.starts_with(&[0x1b, b'[', b'<'])
}

pub fn is_focus_in_out(b: &[u8]) -> bool {
    b.len() == 3 && b[0] == 0x1b && b[1] == b'[' && (b[2] == b'I' || b[2] == b'O')
}

/// Normalize Enter for TUI apps running in PTY raw mode: map `\r` or `\n` → `\r`.
/// TUIs expect a bare carriage return; sending `\r\n` causes a newline insertion instead of submit.
pub fn map_enter(raw: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(raw.len());
    for &b in raw {
        match b {
            b'\r' | b'\n' => v.push(b'\r'),
            _ => v.push(b),
        }
    }
    v
}

pub fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some(x) => {
                out.push('\\');
                out.push(x);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescape_sequences() {
        assert_eq!(unescape(r"a\nb"), "a\nb");
        assert_eq!(unescape(r"a\\b"), "a\\b");
    }

    #[test]
    fn focus_filter() {
        assert!(is_focus_in_out(b"\x1b[I"));
        assert!(is_focus_in_out(b"\x1b[O"));
        assert!(!is_focus_in_out(b"\x1b[1;2H"));
    }

    #[test]
    fn enter_mapping() {
        assert_eq!(map_enter(b"hi\n"), b"hi\r");
        assert_eq!(map_enter(b"hi\r"), b"hi\r");
    }
}
