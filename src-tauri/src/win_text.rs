/// Decodes console output produced by Windows tools (`schtasks`, `tasklist`).
/// They write in the OEM code page, which is CP866 on Russian Windows and not
/// UTF-8.
pub fn decode_oem(bytes: &[u8]) -> String {
    use windows_sys::Win32::Globalization::CP_OEMCP;
    decode_with_codepage(bytes, CP_OEMCP)
}

fn decode_with_codepage(bytes: &[u8], codepage: u32) -> String {
    use windows_sys::Win32::Globalization::MultiByteToWideChar;

    if bytes.is_empty() {
        return String::new();
    }

    let length = unsafe {
        MultiByteToWideChar(
            codepage,
            0,
            bytes.as_ptr(),
            bytes.len() as i32,
            std::ptr::null_mut(),
            0,
        )
    };
    if length <= 0 {
        return String::from_utf8_lossy(bytes).to_string();
    }

    let mut buffer = vec![0u16; length as usize];
    let written = unsafe {
        MultiByteToWideChar(
            codepage,
            0,
            bytes.as_ptr(),
            bytes.len() as i32,
            buffer.as_mut_ptr(),
            length,
        )
    };
    if written <= 0 {
        return String::from_utf8_lossy(bytes).to_string();
    }

    String::from_utf16_lossy(&buffer[..written as usize])
}

#[cfg(test)]
mod tests {
    use super::decode_oem;

    #[test]
    fn decodes_russian_cp866() {
        let bytes = [0x8F, 0xE0, 0xA8, 0xA2, 0xA5, 0xE2];
        // CP_OEMCP depends on the CI machine locale, so the conversion itself
        // is tested with an explicit code page.
        assert_eq!(super::decode_with_codepage(&bytes, 866), "Привет");
    }

    #[test]
    fn keeps_ascii() {
        assert_eq!(decode_oem(b"Ready"), "Ready");
    }
}
