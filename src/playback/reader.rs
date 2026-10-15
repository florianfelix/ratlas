use std::io::{Cursor, Read, Seek, SeekFrom};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// An unseekable stream reader that receives chunks from an async HTTP stream channel
/// and feeds them into rodio's Decoder on the dedicated audio worker thread.
pub struct ChannelReader {
    rx: crossbeam_channel::Receiver<Vec<u8>>,
    current: Cursor<Vec<u8>>,
    bytes_read: u64,
    stop_flag: Arc<AtomicBool>,
    is_probing: Arc<AtomicBool>,
    max_probe_bytes: u64,
}

impl ChannelReader {
    pub fn new(
        rx: crossbeam_channel::Receiver<Vec<u8>>,
        stop_flag: Arc<AtomicBool>,
        is_probing: Arc<AtomicBool>,
        max_probe_bytes: u64,
    ) -> Self {
        Self {
            rx,
            current: Cursor::new(Vec::new()),
            bytes_read: 0,
            stop_flag,
            is_probing,
            max_probe_bytes,
        }
    }
}

impl Read for ChannelReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        loop {
            if self.stop_flag.load(Ordering::SeqCst) {
                return Ok(0); // Signal EOF if playback stopped
            }

            // In probe mode, prevent the demuxer from scanning infinite junk bytes
            if self.is_probing.load(Ordering::SeqCst) && self.bytes_read >= self.max_probe_bytes {
                return Ok(0);
            }

            let n = self.current.read(buf)?;
            if n > 0 {
                self.bytes_read += n as u64;
                return Ok(n);
            }

            // Current chunk exhausted, wait for next network chunk with periodic timeout
            // to recheck stop_flag and probe limits even during network latency.
            match self.rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(chunk) => {
                    self.current = Cursor::new(chunk);
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    continue;
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    return Ok(0); // Channel closed (stream finished or aborted)
                }
            }
        }
    }
}

impl Seek for ChannelReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match pos {
            SeekFrom::Current(0) => Ok(self.bytes_read),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Live audio streams do not support arbitrary seeking",
            )),
        }
    }
}
