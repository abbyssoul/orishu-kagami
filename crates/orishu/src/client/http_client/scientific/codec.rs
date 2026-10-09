//! Preflight the narrow receipt/descriptor response before serde allocates or
//! buffers internally tagged data. This is not the workload's identity codec or
//! the peer membership codec: the fact profile contains only maps, text, unsigned
//! integers and null, plus finite simulation-time floats for command/status IO.
//! No arbitrary API collection is accepted by this client.

#[cfg(test)]
pub(super) fn validate(bytes: &[u8]) -> Result<(), ()> {
    validate_profile(bytes, false)
}
pub(super) fn validate_profile(bytes: &[u8], finite_floats: bool) -> Result<(), ()> {
    if bytes.len() > super::MAX_RESPONSE_BYTES {
        return Err(());
    }
    let mut scan = Scan {
        bytes,
        at: 0,
        items: 256,
        finite_floats,
    };
    scan.value(0)?;
    if scan.at != bytes.len() {
        return Err(());
    }
    Ok(())
}
struct Scan<'a> {
    bytes: &'a [u8],
    at: usize,
    items: usize,
    finite_floats: bool,
}
impl<'a> Scan<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], ()> {
        let end = self.at.checked_add(count).ok_or(())?;
        let bytes = self.bytes.get(self.at..end).ok_or(())?;
        self.at = end;
        Ok(bytes)
    }
    fn head(&mut self) -> Result<(u8, u64), ()> {
        self.items = self.items.checked_sub(1).ok_or(())?;
        let byte = self.take(1)?[0];
        let argument = match byte & 31 {
            n @ 0..=23 => u64::from(n),
            n @ 24..=27 => {
                let mut argument = 0u64;
                for byte in self.take(1 << (n - 24))? {
                    argument = (argument << 8) | u64::from(*byte);
                }
                argument
            }
            31 if byte == 0xbf => u64::MAX,
            _ => return Err(()),
        };
        if byte >> 5 == 7 && byte != 0xf6 {
            let finite = match byte {
                0xf9 => argument & 0x7c00 != 0x7c00,
                0xfa => f32::from_bits(argument as u32).is_finite(),
                0xfb => f64::from_bits(argument).is_finite(),
                _ => false,
            };
            if !self.finite_floats || !finite {
                return Err(());
            }
        }
        Ok((byte >> 5, argument))
    }
    fn text(&mut self, size: u64) -> Result<&'a str, ()> {
        if size > 1024 {
            return Err(());
        }
        std::str::from_utf8(self.take(size as usize)?).map_err(|_| ())
    }
    fn value(&mut self, depth: usize) -> Result<(), ()> {
        if depth > 12 {
            return Err(());
        }
        let indefinite = self.bytes.get(self.at) == Some(&0xbf);
        let (kind, size) = self.head()?;
        match kind {
            0 | 7 => Ok(()),
            3 => self.text(size).map(|_| ()),
            5 => {
                if !indefinite && size > 16 {
                    return Err(());
                }
                let mut keys = [None; 16];
                let mut count = 0;
                loop {
                    if indefinite && self.bytes.get(self.at) == Some(&0xff) {
                        self.take(1)?;
                        return Ok(());
                    }
                    if !indefinite && count == size as usize {
                        return Ok(());
                    }
                    if count == keys.len() {
                        return Err(());
                    }
                    let (kind, size) = self.head()?;
                    if kind != 3 {
                        return Err(());
                    }
                    let key = self.text(size)?;
                    if keys[..count].contains(&Some(key)) {
                        return Err(());
                    }
                    keys[count] = Some(key);
                    self.value(depth + 1)?;
                    count += 1;
                }
            }
            _ => Err(()),
        }
    }
}
