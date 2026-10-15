/// Helper to detect if a buffer starts with or contains an ADTS AAC stream.
pub fn is_adts_stream(buf: &[u8]) -> bool {
    let search_limit = buf.len().min(4096);
    for i in 0..search_limit.saturating_sub(6) {
        if buf[i] == 0xFF && (buf[i + 1] & 0xF6) == 0xF0 {
            let sr_idx = (buf[i + 2] >> 2) & 0x0F;
            let ch = ((buf[i + 2] & 1) << 2) | (buf[i + 3] >> 6);
            let frame_len = (((buf[i + 3] & 0x03) as usize) << 11)
                | ((buf[i + 4] as usize) << 3)
                | ((buf[i + 5] >> 5) as usize);

            if sr_idx < 13 && ch > 0 && (7..=4096).contains(&frame_len) {
                if i + frame_len + 1 < buf.len() {
                    let next = i + frame_len;
                    if buf[next] == 0xFF && (buf[next + 1] & 0xF6) == 0xF0 {
                        return true;
                    }
                } else {
                    return true;
                }
            }
        }
    }
    false
}

/// Finds the offset of the first valid ADTS frame in `buf`.
pub fn find_first_adts_frame(buf: &[u8]) -> Option<usize> {
    let search_limit = buf.len().min(8192);
    for i in 0..search_limit.saturating_sub(6) {
        if buf[i] == 0xFF && (buf[i + 1] & 0xF6) == 0xF0 {
            let sr_idx = (buf[i + 2] >> 2) & 0x0F;
            let ch = ((buf[i + 2] & 1) << 2) | (buf[i + 3] >> 6);
            let frame_len = (((buf[i + 3] & 0x03) as usize) << 11)
                | ((buf[i + 4] as usize) << 3)
                | ((buf[i + 5] >> 5) as usize);

            if sr_idx < 13 && ch > 0 && (7..=4096).contains(&frame_len) {
                if i + frame_len + 1 < buf.len() {
                    let next = i + frame_len;
                    if buf[next] == 0xFF && (buf[next + 1] & 0xF6) == 0xF0 {
                        return Some(i);
                    }
                } else {
                    return Some(i);
                }
            }
        }
    }
    None
}

/// Normalizes MPEG-2 AAC ADTS sync headers (0xFFF9 / 0xFFF8) to MPEG-4 AAC headers (0xFFF1 / 0xFFF0).
/// Symphonia 0.5's AdtsReader hardcodes 0xFFF1 and rejects 0xFFF9.
/// By following frame lengths, this modifies only the second byte of verified ADTS frame headers
/// and preserves audio payloads unmodified.
#[derive(Debug, Default)]
pub struct AdtsNormalizer {
    bytes_remaining_in_frame: usize,
    header_carry: Vec<u8>,
}

impl AdtsNormalizer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn process(&mut self, chunk: &mut [u8]) {
        let mut i = 0;
        let len = chunk.len();

        // 1. If we had an incomplete header carrying over from the previous chunk
        if !self.header_carry.is_empty() {
            let needed = 7usize.saturating_sub(self.header_carry.len());
            let take = needed.min(len);
            self.header_carry.extend_from_slice(&chunk[..take]);
            i += take;

            // If the second byte of the syncword fell in this chunk (e.g. carry was just [0xFF]),
            // normalize it directly in chunk[0]
            if self.header_carry.len() >= 2 {
                if (self.header_carry[1] & 0xF6) == 0xF0 && (self.header_carry[1] & 0x08) != 0 {
                    self.header_carry[1] &= !0x08;
                    if take > 0 && (self.header_carry.len() - take) < 2 {
                        let chunk_idx = 1 - (self.header_carry.len() - take);
                        chunk[chunk_idx] &= !0x08;
                    }
                }
            }

            if self.header_carry.len() >= 6 {
                let frame_len = (((self.header_carry[3] & 0x03) as usize) << 11)
                    | ((self.header_carry[4] as usize) << 3)
                    | ((self.header_carry[5] >> 5) as usize);

                let consumed = self.header_carry.len();
                self.header_carry.clear();

                if frame_len >= 7 {
                    let remaining_in_frame = frame_len.saturating_sub(consumed);
                    let skip = remaining_in_frame.min(len - i);
                    self.bytes_remaining_in_frame = remaining_in_frame - skip;
                    i += skip;
                }
            } else {
                return;
            }
        }

        // 2. Skip payload bytes belonging to the current frame
        if self.bytes_remaining_in_frame > 0 {
            let skip = self.bytes_remaining_in_frame.min(len - i);
            self.bytes_remaining_in_frame -= skip;
            i += skip;
        }

        // 3. Scan for and normalize subsequent ADTS frames
        while i < len {
            if chunk[i] == 0xFF {
                if i + 1 < len {
                    if (chunk[i + 1] & 0xF6) == 0xF0 {
                        // Normalize MPEG-2 AAC ID bit to MPEG-4 AAC
                        chunk[i + 1] &= !0x08;

                        if i + 6 <= len {
                            let frame_len = (((chunk[i + 3] & 0x03) as usize) << 11)
                                | ((chunk[i + 4] as usize) << 3)
                                | ((chunk[i + 5] >> 5) as usize);

                            if frame_len >= 7 {
                                let remaining = len - i;
                                if frame_len <= remaining {
                                    i += frame_len;
                                    continue;
                                } else {
                                    self.bytes_remaining_in_frame = frame_len - remaining;
                                    break;
                                }
                            }
                        } else {
                            // Header is split at chunk end
                            self.header_carry = chunk[i..len].to_vec();
                            break;
                        }
                    }
                } else {
                    // Chunk ends right on the 0xFF sync byte
                    self.header_carry = vec![0xFF];
                    break;
                }
            }
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_adts_stream_detection() {
        let dummy = vec![0x00; 100];
        assert!(!is_adts_stream(&dummy));

        // Create two valid ADTS frames (sample rate 44.1k = 4, 2 channels, flen = 100)
        let mut frame1 = vec![0x00; 100];
        frame1[0] = 0xFF;
        frame1[1] = 0xF9; // MPEG-2 AAC
        frame1[2] = 0x50; // profile=1 (LC), sr_idx=4
        frame1[3] = 0x80; // ch=2, len top bits = 0
        frame1[4] = 100 >> 3;
        frame1[5] = (100 & 0x07) << 5;

        let mut frame2 = vec![0x00; 100];
        frame2[0] = 0xFF;
        frame2[1] = 0xF9;
        frame2[2] = 0x50;
        frame2[3] = 0x80;
        frame2[4] = 100 >> 3;
        frame2[5] = (100 & 0x07) << 5;

        let mut test_buf = vec![0x00; 100]; // 100 bytes of leading junk
        test_buf.extend_from_slice(&frame1);
        test_buf.extend_from_slice(&frame2);

        assert!(is_adts_stream(&test_buf));
        assert_eq!(find_first_adts_frame(&test_buf), Some(100));
    }

    #[test]
    fn test_adts_normalizer_chunked() {
        use std::fs;
        if let Ok(data) = fs::read("target/raw_canela.aac") {
            let mut normalizer = AdtsNormalizer::new();
            let mut normalized = Vec::new();

            // Feed in small arbitrary chunk sizes to stress test boundary handling
            for chunk_slice in data.chunks(73) {
                let mut c = chunk_slice.to_vec();
                normalizer.process(&mut c);
                normalized.extend_from_slice(&c);
            }

            let cursor = Cursor::new(normalized);
            let res = rodio::Decoder::builder()
                .with_data(cursor)
                .with_hint("aac")
                .with_mime_type("audio/aac")
                .build();

            match res {
                Ok(decoder) => {
                    use rodio::Source;
                    println!(
                        "Successfully decoded chunk-normalized AAC: channels={}, sample_rate={}",
                        decoder.channels(),
                        decoder.sample_rate()
                    );
                    assert_eq!(decoder.channels().get(), 2);
                }
                Err(e) => {
                    panic!("Decoder failed on chunk-normalized ADTS: {:?}", e);
                }
            }
        }
    }

    #[test]
    fn test_adts_normalizer_ondacero() {
        use std::fs;
        if let Ok(data) = fs::read("target/raw_ondacero.aac") {
            let mut normalizer = AdtsNormalizer::new();
            let mut normalized = Vec::new();

            // Feed in small arbitrary chunk sizes to stress test boundary handling
            for chunk_slice in data.chunks(97) {
                let mut c = chunk_slice.to_vec();
                normalizer.process(&mut c);
                normalized.extend_from_slice(&c);
            }

            let cursor = Cursor::new(normalized);
            let res = rodio::Decoder::builder()
                .with_data(cursor)
                .with_hint("aac")
                .with_mime_type("audio/aac")
                .build();

            match res {
                Ok(decoder) => {
                    use rodio::Source;
                    println!(
                        "Successfully decoded chunk-normalized Onda Cero: channels={}, sample_rate={}",
                        decoder.channels(),
                        decoder.sample_rate()
                    );
                    assert_eq!(decoder.channels().get(), 2);
                }
                Err(e) => {
                    panic!("Decoder failed on chunk-normalized Onda Cero: {:?}", e);
                }
            }
        }
    }
}
