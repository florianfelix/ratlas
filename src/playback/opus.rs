use ogg::reading::PacketReader;
use rodio::Source;
use rusty_opus::OpusDecoder;
use std::io::{Read, Seek};
use std::num::{NonZeroU16, NonZeroU32};
use std::time::Duration;

/// Errors that can occur when reading or decoding an Ogg Opus stream.
#[derive(Debug)]
pub enum OpusError {
    Ogg(ogg::OggReadError),
    Codec(rusty_opus::Error),
    InvalidHeader(&'static str),
    UnsupportedChannels(u8),
    UnexpectedEof,
}

impl std::fmt::Display for OpusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ogg(e) => write!(f, "I/O or OGG container error: {e}"),
            Self::Codec(e) => write!(f, "Opus codec error: {e:?}"),
            Self::InvalidHeader(msg) => write!(f, "Invalid Opus header: {msg}"),
            Self::UnsupportedChannels(c) => {
                write!(
                    f,
                    "Unsupported channel count: {c} (only mono and stereo are supported)"
                )
            }
            Self::UnexpectedEof => write!(f, "Unexpected end of stream while reading Opus headers"),
        }
    }
}

impl std::error::Error for OpusError {}

impl From<ogg::OggReadError> for OpusError {
    fn from(e: ogg::OggReadError) -> Self {
        Self::Ogg(e)
    }
}

/// A `rodio::Source` that demuxes an Ogg container and decodes Opus audio packets
/// using the pure-Rust `rusty-opus` codec.
pub struct OpusSource<R: Read + Seek> {
    reader: PacketReader<R>,
    decoder: OpusDecoder,
    stream_serial: u32,
    channels: NonZeroU16,
    sample_rate: NonZeroU32,
    pre_skip: usize,
    gain: f32,

    // Preallocated buffer for raw decoded PCM samples (max 120ms at 48kHz = 5760 samples/ch)
    decode_buf: Vec<f32>,
    // Queue of ready samples waiting to be yielded by Iterator::next()
    sample_buf: Vec<f32>,
    sample_pos: usize,
    exhausted: bool,
}

impl<R: Read + Seek> OpusSource<R> {
    /// Attempts to parse the Ogg Opus identification and comment headers from `reader`
    /// and initialize the `OpusDecoder`.
    pub fn new(reader: R) -> Result<Self, OpusError> {
        let mut packet_reader = PacketReader::new(reader);

        // Find the OpusHead packet (must be the first packet of the Opus logical stream)
        let (head_packet, stream_serial) = loop {
            let p = packet_reader
                .read_packet()?
                .ok_or(OpusError::UnexpectedEof)?;
            if p.data.starts_with(b"OpusHead") {
                let serial = p.stream_serial();
                break (p, serial);
            }
        };

        if head_packet.data.len() < 19 {
            return Err(OpusError::InvalidHeader("OpusHead packet is too short"));
        }

        let channel_count = head_packet.data[9];
        if channel_count == 0 || channel_count > 2 {
            return Err(OpusError::UnsupportedChannels(channel_count));
        }

        let channels = NonZeroU16::new(channel_count as u16)
            .ok_or(OpusError::UnsupportedChannels(channel_count))?;

        let pre_skip = u16::from_le_bytes([head_packet.data[10], head_packet.data[11]]) as usize;
        let output_gain_q8 = i16::from_le_bytes([head_packet.data[16], head_packet.data[17]]);
        let gain = if output_gain_q8 == 0 {
            1.0f32
        } else {
            10.0f32.powf(output_gain_q8 as f32 / (20.0 * 256.0))
        };

        // Opus always decodes to 48000 Hz internally
        let sample_rate = NonZeroU32::new(48000).expect("48000 is non-zero");

        // The second packet in the logical stream is OpusTags
        loop {
            let p = packet_reader
                .read_packet()?
                .ok_or(OpusError::UnexpectedEof)?;
            if p.stream_serial() == stream_serial {
                if !p.data.starts_with(b"OpusTags") {
                    return Err(OpusError::InvalidHeader(
                        "Expected OpusTags packet after OpusHead",
                    ));
                }
                break;
            }
        }

        let decoder = OpusDecoder::new(48000, channel_count as usize).map_err(OpusError::Codec)?;

        let max_samples_per_frame = 5760 * (channel_count as usize);

        Ok(Self {
            reader: packet_reader,
            decoder,
            stream_serial,
            channels,
            sample_rate,
            pre_skip,
            gain,
            decode_buf: vec![0.0f32; max_samples_per_frame],
            sample_buf: Vec::with_capacity(max_samples_per_frame),
            sample_pos: 0,
            exhausted: false,
        })
    }

    /// Read and decode the next Opus audio packet into `self.sample_buf`.
    fn fill_buffer(&mut self) -> bool {
        if self.exhausted {
            return false;
        }

        self.sample_buf.clear();
        self.sample_pos = 0;

        let channel_count = self.channels.get() as usize;

        while self.sample_buf.is_empty() {
            let packet = match self.reader.read_packet() {
                Ok(Some(p)) => p,
                Ok(None) => {
                    self.exhausted = true;
                    return false;
                }
                Err(e) => {
                    tracing::warn!("Ogg container read error in Opus stream: {e:?}");
                    self.exhausted = true;
                    return false;
                }
            };

            // Ignore packets from other streams if multiplexed
            if packet.stream_serial() != self.stream_serial || packet.data.is_empty() {
                continue;
            }

            match self
                .decoder
                .decode(&packet.data, 5760, &mut self.decode_buf)
            {
                Ok(samples_per_ch) => {
                    let total_samples = samples_per_ch * channel_count;
                    let decoded_slice = &self.decode_buf[..total_samples];

                    if self.pre_skip > 0 {
                        let discard_frames = self.pre_skip.min(samples_per_ch);
                        self.pre_skip -= discard_frames;
                        let discard_samples = discard_frames * channel_count;

                        if discard_samples < total_samples {
                            let valid_slice = &decoded_slice[discard_samples..];
                            if (self.gain - 1.0).abs() > 0.0001 {
                                self.sample_buf
                                    .extend(valid_slice.iter().map(|&s| s * self.gain));
                            } else {
                                self.sample_buf.extend_from_slice(valid_slice);
                            }
                        }
                    } else if (self.gain - 1.0).abs() > 0.0001 {
                        self.sample_buf
                            .extend(decoded_slice.iter().map(|&s| s * self.gain));
                    } else {
                        self.sample_buf.extend_from_slice(decoded_slice);
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Opus decode error on packet (len {}): {e:?}",
                        packet.data.len()
                    );
                    // On corrupted packet in live stream, attempt packet-loss concealment (PLC)
                    if let Ok(samples_per_ch) = self.decoder.decode(&[], 960, &mut self.decode_buf)
                    {
                        let total_samples = samples_per_ch * channel_count;
                        self.sample_buf
                            .extend_from_slice(&self.decode_buf[..total_samples]);
                    }
                }
            }
        }

        !self.sample_buf.is_empty()
    }
}

impl<R: Read + Seek + Send> Iterator for OpusSource<R> {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.sample_pos < self.sample_buf.len() {
            let sample = self.sample_buf[self.sample_pos];
            self.sample_pos += 1;
            return Some(sample);
        }

        if self.fill_buffer() {
            let sample = self.sample_buf[self.sample_pos];
            self.sample_pos += 1;
            Some(sample)
        } else {
            None
        }
    }
}

impl<R: Read + Seek + Send> Source for OpusSource<R> {
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        let remaining = self.sample_buf.len().saturating_sub(self.sample_pos);
        if remaining > 0 { Some(remaining) } else { None }
    }

    #[inline]
    fn channels(&self) -> NonZeroU16 {
        self.channels
    }

    #[inline]
    fn sample_rate(&self) -> NonZeroU32 {
        self.sample_rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_opus_source_with_channel_reader() {
        use crate::playback::reader::ChannelReader;
        use std::sync::Arc;
        use std::sync::atomic::AtomicBool;

        let Ok(data) = std::fs::read("/tmp/alands.ogg") else {
            return;
        };
        let (tx, rx) = crossbeam_channel::bounded(16);

        // Send data in chunks of 4 KB
        for chunk in data.chunks(4096) {
            tx.send(chunk.to_vec()).unwrap();
        }
        drop(tx);

        let stop = Arc::new(AtomicBool::new(false));
        let probing = Arc::new(AtomicBool::new(false));
        let reader = ChannelReader::new(rx, stop, probing, 64 * 1024);

        let source = OpusSource::new(reader).expect("create OpusSource with ChannelReader");
        assert_eq!(source.channels().get(), 2);
        assert_eq!(source.sample_rate().get(), 48000);

        let mut count = 0;
        for _ in source {
            count += 1;
        }

        println!("Decoded total samples from channel reader: {}", count);
        assert!(count > 48000); // More than 1 second of stereo audio
    }

    #[test]
    fn test_opus_source_send_and_static() {
        fn assert_send_static<T: rodio::Source + Send + 'static>() {}
        assert_send_static::<OpusSource<std::io::Cursor<Vec<u8>>>>();
    }

    use super::*;
    use std::fs::File;

    #[test]
    fn test_opus_source_playback() {
        let Ok(file) = File::open("/tmp/alands.ogg") else {
            return;
        };
        let mut source = OpusSource::new(file).expect("initialize OpusSource");

        assert_eq!(source.channels().get(), 2);
        assert_eq!(source.sample_rate().get(), 48000);

        // Read 1 second of audio samples (48000 samples * 2 channels = 96000 samples)
        let mut sample_count = 0;
        for _ in 0..96000 {
            if let Some(sample) = source.next() {
                assert!((-2.0..=2.0).contains(&sample));
                sample_count += 1;
            } else {
                break;
            }
        }

        println!("OpusSource read {} samples", sample_count);
        assert!(sample_count > 0);
    }
}
