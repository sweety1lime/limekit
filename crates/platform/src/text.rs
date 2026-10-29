//! Text in the encodings Windows files come in.

/// Decodes bytes written in the system's ANSI code page, which is what Windows
/// assumes for a file without a byte order mark, as many `.inf` files are. Off
/// Windows the bytes are read as Latin-1, which at least keeps ASCII intact.
pub fn from_ansi(bytes: &[u8]) -> String {
    #[cfg(windows)]
    {
        win32::from_ansi(bytes)
    }
    #[cfg(not(windows))]
    {
        bytes.iter().map(|&byte| char::from(byte)).collect()
    }
}

#[cfg(windows)]
mod win32 {
    use windows::Win32::Globalization::{
        CP_ACP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS, MultiByteToWideChar,
    };

    pub fn from_ansi(bytes: &[u8]) -> String {
        if bytes.is_empty() {
            return String::new();
        }
        let flags = MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0);
        // SAFETY: both calls only read `bytes` and write into `wide`, which is
        // as long as the first call said it has to be.
        unsafe {
            let length = MultiByteToWideChar(CP_ACP, flags, bytes, None);
            if length <= 0 {
                return String::from_utf8_lossy(bytes).into_owned();
            }
            let mut wide = vec![0u16; length as usize];
            let written = MultiByteToWideChar(CP_ACP, flags, bytes, Some(&mut wide));
            String::from_utf16_lossy(&wide[..written.max(0) as usize])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_comes_through_whatever_the_code_page_is() {
        assert_eq!(
            from_ansi(b"[Strings]\r\nCUR_DIR = \"Cursors\\x\""),
            "[Strings]\r\nCUR_DIR = \"Cursors\\x\""
        );
        assert_eq!(from_ansi(b""), "");
    }
}
