//! Converts WMI/DMTF timestamps (`20261106114725.774512-000`) to ISO 8601 so
//! the UI can parse them with `new Date(...)`.

/// Returns `YYYY-MM-DDTHH:MM:SS[.mmm][+HH:MM]`, or `None` when the input is not
/// a WMI datetime.
pub fn to_iso(raw: &str) -> Option<String> {
    if !raw.is_ascii() || raw.len() < 14 || !raw.as_bytes()[..14].iter().all(u8::is_ascii_digit) {
        return None;
    }

    let number = |from: usize, to: usize| raw.get(from..to)?.parse::<u32>().ok();
    let year = number(0, 4)?;
    let month = number(4, 6)?;
    let day = number(6, 8)?;
    let hour = number(8, 10)?;
    let minute = number(10, 12)?;
    let second = number(12, 14)?;

    let mut out = format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}");
    let mut rest = &raw[14..];

    if let Some(fraction) = rest.strip_prefix('.') {
        let digits: String = fraction.chars().take_while(char::is_ascii_digit).collect();
        if !digits.is_empty() {
            let mut millis = digits.clone();
            millis.truncate(3);
            while millis.len() < 3 {
                millis.push('0');
            }
            out.push('.');
            out.push_str(&millis);
            rest = &fraction[digits.len()..];
        }
    }

    if let Some(sign @ ('+' | '-')) = rest.chars().next() {
        let digits: String = rest[1..].chars().take_while(char::is_ascii_digit).collect();
        if !digits.is_empty() {
            let minutes = digits.parse::<i32>().unwrap_or(0);
            let offset = if sign == '-' { -minutes } else { minutes };
            out.push_str(&format!(
                "{}{:02}:{:02}",
                if offset < 0 { '-' } else { '+' },
                offset.abs() / 60,
                offset.abs() % 60
            ));
        }
    }

    Some(out)
}

#[cfg(test)]
mod tests {
    use super::to_iso;

    #[test]
    fn converts_utc_offset_zero() {
        assert_eq!(
            to_iso("20261106114725.774512-000").as_deref(),
            Some("2026-11-06T11:47:25.774+00:00")
        );
    }

    #[test]
    fn converts_positive_offset() {
        assert_eq!(
            to_iso("20261106114725.774512+180").as_deref(),
            Some("2026-11-06T11:47:25.774+03:00")
        );
    }

    #[test]
    fn converts_without_fraction_and_offset() {
        assert_eq!(
            to_iso("20261106114725").as_deref(),
            Some("2026-11-06T11:47:25")
        );
    }

    #[test]
    fn rejects_non_wmi_input() {
        assert_eq!(to_iso("2026-11-06T11:47:25Z"), None);
        assert_eq!(to_iso(""), None);
    }
}
