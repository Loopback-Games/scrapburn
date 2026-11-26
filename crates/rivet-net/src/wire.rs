//! Little-endian byte codec. Every read is bounds-checked and returns an error instead of
//! panicking, because the bytes come from the network.

/// A message being encoded.
#[derive(Debug, Default, Clone)]
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.buf.push(v);
        self
    }

    pub fn bool(&mut self, v: bool) -> &mut Self {
        self.u8(v as u8)
    }

    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn f32(&mut self, v: f32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn f64(&mut self, v: f64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    /// A UTF-8 string of at most 255 bytes (longer strings are cut at a character boundary).
    pub fn str(&mut self, s: &str) -> &mut Self {
        let mut end = s.len().min(255);
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        self.u8(end as u8);
        self.buf.extend_from_slice(&s.as_bytes()[..end]);
        self
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn finish(self) -> Vec<u8> {
        self.buf
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WireError {
    #[error("message truncated")]
    Truncated,
    #[error("invalid {0}")]
    Invalid(&'static str),
}

/// A message being decoded.
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    buf: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, at: 0 }
    }

    fn take<const N: usize>(&mut self) -> Result<[u8; N], WireError> {
        let end = self.at.checked_add(N).ok_or(WireError::Truncated)?;
        let bytes = self.buf.get(self.at..end).ok_or(WireError::Truncated)?;
        self.at = end;
        let mut out = [0u8; N];
        out.copy_from_slice(bytes);
        Ok(out)
    }

    pub fn u8(&mut self) -> Result<u8, WireError> {
        Ok(self.take::<1>()?[0])
    }

    pub fn bool(&mut self) -> Result<bool, WireError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(WireError::Invalid("bool")),
        }
    }

    pub fn u16(&mut self) -> Result<u16, WireError> {
        Ok(u16::from_le_bytes(self.take()?))
    }

    pub fn u32(&mut self) -> Result<u32, WireError> {
        Ok(u32::from_le_bytes(self.take()?))
    }

    pub fn u64(&mut self) -> Result<u64, WireError> {
        Ok(u64::from_le_bytes(self.take()?))
    }

    /// A finite `f32` (NaN and infinities are rejected: they would poison the simulation).
    pub fn f32(&mut self) -> Result<f32, WireError> {
        let v = f32::from_le_bytes(self.take()?);
        if v.is_finite() {
            Ok(v)
        } else {
            Err(WireError::Invalid("non-finite f32"))
        }
    }

    /// A finite `f64`.
    pub fn f64(&mut self) -> Result<f64, WireError> {
        let v = f64::from_le_bytes(self.take()?);
        if v.is_finite() {
            Ok(v)
        } else {
            Err(WireError::Invalid("non-finite f64"))
        }
    }

    pub fn str(&mut self) -> Result<&'a str, WireError> {
        let n = self.u8()? as usize;
        let end = self.at + n;
        let bytes = self.buf.get(self.at..end).ok_or(WireError::Truncated)?;
        self.at = end;
        std::str::from_utf8(bytes).map_err(|_| WireError::Invalid("utf-8"))
    }

    /// The bytes not read yet.
    pub fn rest(&self) -> &'a [u8] {
        &self.buf[self.at.min(self.buf.len())..]
    }

    pub fn is_empty(&self) -> bool {
        self.at >= self.buf.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut w = Writer::new();
        w.u8(7)
            .bool(true)
            .u16(513)
            .u32(70_000)
            .f32(-1.5)
            .f64(2.25)
            .str("nail");
        let bytes = w.finish();
        let mut r = Reader::new(&bytes);
        assert_eq!(r.u8(), Ok(7));
        assert_eq!(r.bool(), Ok(true));
        assert_eq!(r.u16(), Ok(513));
        assert_eq!(r.u32(), Ok(70_000));
        assert_eq!(r.f32(), Ok(-1.5));
        assert_eq!(r.f64(), Ok(2.25));
        assert_eq!(r.str(), Ok("nail"));
        assert!(r.is_empty());
    }

    #[test]
    fn truncated_and_invalid_input_errors_instead_of_panicking() {
        assert_eq!(Reader::new(&[1, 2]).u32(), Err(WireError::Truncated));
        assert_eq!(Reader::new(&[5, b'a']).str(), Err(WireError::Truncated));
        assert_eq!(
            Reader::new(&[2, 0xff, 0xfe]).str(),
            Err(WireError::Invalid("utf-8"))
        );
        assert_eq!(Reader::new(&[2]).bool(), Err(WireError::Invalid("bool")));
        let nan = f32::NAN.to_le_bytes();
        assert!(Reader::new(&nan).f32().is_err());
        let inf = f64::INFINITY.to_le_bytes();
        assert!(Reader::new(&inf).f64().is_err());
    }

    #[test]
    fn long_strings_are_cut_on_a_char_boundary() {
        let s = "é".repeat(200); // 400 bytes
        let mut w = Writer::new();
        w.str(&s);
        let bytes = w.finish();
        let got = Reader::new(&bytes).str().unwrap().to_string();
        assert!(got.len() <= 255 && s.starts_with(&got));
    }
}
