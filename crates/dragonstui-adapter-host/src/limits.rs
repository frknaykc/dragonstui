use std::io::{self, BufRead, Read, Write};
use std::time::{Duration, Instant};

pub(crate) const MESSAGE_BYTES: usize = 1024 * 1024;
pub(crate) const MANIFEST_BYTES: usize = 64 * 1024;
pub(crate) const STDERR_BYTES: usize = 4096;
pub(crate) const MAX_TIMEOUT: Duration = Duration::from_secs(300);

pub(crate) fn timeout(value: Duration) -> Duration {
    value.min(MAX_TIMEOUT)
}

// Includes LF (and CR, when present) in the wire byte budget. Reads at most
// limit + 1 bytes even when no delimiter is available.
pub(crate) fn read_line(reader: &mut impl BufRead, limit: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_until(b'\n', &mut bytes)?;
    if bytes.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "adapter line byte limit exceeded",
        ));
    }
    Ok(bytes)
}

pub(crate) fn diagnostic_line(
    reader: &mut impl BufRead,
    limit: usize,
) -> io::Result<Option<String>> {
    let mut bytes = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_until(b'\n', &mut bytes)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    if bytes.len() <= limit {
        return Ok(Some(
            String::from_utf8_lossy(&bytes)
                .trim_end_matches(['\r', '\n'])
                .to_owned(),
        ));
    }
    if bytes.last() != Some(&b'\n') {
        loop {
            let chunk = reader.fill_buf()?;
            if chunk.is_empty() {
                break;
            }
            let end = chunk.iter().position(|byte| *byte == b'\n');
            let count = end.map_or(chunk.len(), |index| index + 1);
            reader.consume(count);
            if end.is_some() {
                break;
            }
        }
    }
    Ok(Some("[stderr line byte limit exceeded]".to_owned()))
}

pub(crate) struct LimitedWriter {
    pub(crate) bytes: Vec<u8>,
    limit: usize,
}

impl LimitedWriter {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
        }
    }
}

impl Write for LimitedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "adapter message byte limit exceeded",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// Fixed one-second windows measured at ingress, not at UI consumption.
// Windows may admit two bursts across a boundary; this is not a sliding window.
pub(crate) struct EventRate {
    start: Instant,
    used: usize,
    limit: usize,
}
impl EventRate {
    pub(crate) fn new(limit: usize, now: Instant) -> Self {
        Self {
            start: now,
            used: 0,
            limit,
        }
    }
    pub(crate) fn admit(&mut self, now: Instant) -> bool {
        if now.saturating_duration_since(self.start) >= Duration::from_secs(1) {
            self.start = now;
            self.used = 0;
        }
        if self.used >= self.limit {
            return false;
        }
        self.used += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn line_budget_includes_delimiter_and_bounds_unterminated_input() {
        assert_eq!(read_line(&mut &b"abc\n"[..], 4).unwrap(), b"abc\n");
        assert!(read_line(&mut &b"abcd\n"[..], 4).is_err());
        let mut input = &b"abcdef"[..];
        assert!(read_line(&mut input, 4).is_err());
        assert_eq!(input, b"f");
        assert!(read_line(&mut &b""[..], 4).unwrap().is_empty());
    }
    #[test]
    fn writer_rejects_without_partial_append() {
        let mut writer = LimitedWriter::new(4);
        writer.write_all(b"abcd").unwrap();
        assert!(writer.write_all(b"e").is_err());
        assert_eq!(writer.bytes, b"abcd");
    }

    #[test]
    fn diagnostic_truncation_does_not_consume_the_following_line() {
        for bytes in [&b"abcd\nok\n"[..], &b"abcdef\nok\n"[..]] {
            let mut reader = bytes;
            assert_eq!(
                diagnostic_line(&mut reader, 4).unwrap().unwrap(),
                "[stderr line byte limit exceeded]"
            );
            assert_eq!(diagnostic_line(&mut reader, 4).unwrap().unwrap(), "ok");
            assert_eq!(diagnostic_line(&mut reader, 4).unwrap(), None);
        }
    }
    #[test]
    fn rate_budget_is_per_instance_and_resets_at_boundary() {
        let now = Instant::now();
        let mut rate = EventRate::new(2, now);
        assert!(rate.admit(now));
        assert!(rate.admit(now));
        assert!(!rate.admit(now));
        assert!(!rate.admit(now + Duration::from_millis(999)));
        assert!(EventRate::new(2, now).admit(now));
        assert!(rate.admit(now + Duration::from_secs(1)));
        assert!(!EventRate::new(0, now).admit(now));
    }
    #[test]
    fn timeouts_are_finite_and_zero_remains_nonblocking() {
        assert_eq!(timeout(Duration::MAX), MAX_TIMEOUT);
        assert_eq!(timeout(Duration::ZERO), Duration::ZERO);
        assert_eq!(timeout(Duration::from_secs(2)), Duration::from_secs(2));
    }
}
