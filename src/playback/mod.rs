use std::io::{Cursor, Read, Seek, SeekFrom};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::runtime::Builder;
use tokio::sync::mpsc;
use winit::event_loop::EventLoopProxy;

use crate::app::application::ProxyMessage;

#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackStatus {
    Stopped,
    Loading { url: String, name: String },
    Playing { url: String, name: String },
    Paused { url: String, name: String },
    Error { message: String },
}

#[derive(Debug, Clone)]
pub enum PlaybackMessage {
    Play { url: String, name: String },
    Stop,
    Pause,
    Resume,
    TogglePlayPause,
    SetVolume(f32),
}

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

pub struct PlaybackWorker {
    proxy: EventLoopProxy<ProxyMessage>,
    current_status: PlaybackStatus,
    current_volume: f32,
    player: Option<rodio::Player>,
    sink_handle: Option<rodio::stream::MixerDeviceSink>,
    // Cancellation flag for the background HTTP streaming task
    stop_stream: Arc<AtomicBool>,
}

impl PlaybackWorker {
    pub fn new(proxy: EventLoopProxy<ProxyMessage>) -> Self {
        let (sink_handle, player) = match rodio::stream::DeviceSinkBuilder::open_default_sink() {
            Ok(mut sink) => {
                sink.log_on_drop(false);
                let player = rodio::Player::connect_new(sink.mixer());
                (Some(sink), Some(player))
            }
            Err(e) => {
                tracing::warn!(
                    "Playback worker: Failed to open default audio output device: {e:?}"
                );
                (None, None)
            }
        };

        Self {
            proxy,
            current_status: PlaybackStatus::Stopped,
            current_volume: 1.0,
            player,
            sink_handle,
            stop_stream: Arc::new(AtomicBool::new(false)),
        }
    }

    fn update_status(&mut self, status: PlaybackStatus) {
        self.current_status = status.clone();
        let _ = self
            .proxy
            .send_event(ProxyMessage::PlaybackStatusChanged(status));
    }

    fn stop_current(&mut self) {
        self.stop_stream.store(true, Ordering::SeqCst);
        if let Some(player) = &self.player {
            player.stop();
            player.clear();
        }
    }

    pub async fn run(&mut self, mut rx: mpsc::UnboundedReceiver<PlaybackMessage>) {
        tracing::info!("Playback worker thread started");

        let mut pending_msg: Option<PlaybackMessage> = None;

        loop {
            let msg = if let Some(m) = pending_msg.take() {
                m
            } else {
                match rx.recv().await {
                    Some(m) => m,
                    None => break,
                }
            };

            match msg {
                PlaybackMessage::Play { url, name } => {
                    self.stop_current();

                    // Run handle_play, but interrupt immediately if a new user action arrives
                    tokio::select! {
                        _ = self.handle_play(url, name) => {}
                        Some(interrupted_msg) = rx.recv() => {
                            tracing::info!("Interrupted current playback connection with new message");
                            self.stop_current();
                            pending_msg = Some(interrupted_msg);
                        }
                    }
                }
                PlaybackMessage::Stop => {
                    self.handle_stop();
                }
                PlaybackMessage::Pause => {
                    self.handle_pause();
                }
                PlaybackMessage::Resume => {
                    self.handle_resume();
                }
                PlaybackMessage::TogglePlayPause => {
                    self.handle_toggle_play_pause();
                }
                PlaybackMessage::SetVolume(vol) => {
                    let clamped = vol.clamp(0.0, 1.0);
                    self.current_volume = clamped;
                    if let Some(player) = &self.player {
                        player.set_volume(clamped);
                    }
                    tracing::info!("Playback volume set to {:.2}", clamped);
                }
            }
        }

        tracing::info!("Playback worker thread shutting down");
    }

    async fn handle_play(&mut self, url: String, name: String) {
        tracing::info!(
            "Playback worker: Connecting to audio stream for '{}' ({})",
            name,
            url
        );

        // Abort previous background stream
        self.stop_stream.store(true, Ordering::SeqCst);
        let stop_flag = Arc::new(AtomicBool::new(false));
        self.stop_stream = stop_flag.clone();

        if let Some(player) = &self.player {
            player.stop();
            player.clear();
        }

        self.update_status(PlaybackStatus::Loading {
            url: url.clone(),
            name: name.clone(),
        });

        // Ensure audio device is initialized
        if self.sink_handle.is_none() {
            match rodio::stream::DeviceSinkBuilder::open_default_sink() {
                Ok(mut sink) => {
                    sink.log_on_drop(false);
                    let player = rodio::Player::connect_new(sink.mixer());
                    player.set_volume(self.current_volume);
                    self.player = Some(player);
                    self.sink_handle = Some(sink);
                }
                Err(e) => {
                    let err_msg = format!("Audio device unavailable: {e:?}");
                    tracing::error!("{err_msg}");
                    self.update_status(PlaybackStatus::Error { message: err_msg });
                    return;
                }
            }
        }

        // Connect to HTTP stream with redirect following and connect/read timeouts.
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .read_timeout(std::time::Duration::from_secs(15))
            .user_agent("Ratlas/0.1.0 (RadioAtlas Audio Streamer)")
            .build()
            .unwrap_or_default();

        let mut response = match client
            .get(&url)
            .header("Icy-MetaData", "0")
            .header("Accept", "*/*")
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                let err_msg = format!("Stream connection failed: {e}");
                tracing::error!("{err_msg}");
                self.update_status(PlaybackStatus::Error { message: err_msg });
                return;
            }
        };

        if !response.status().is_success() {
            let err_msg = format!("HTTP error: {}", response.status());
            tracing::error!("{err_msg}");
            self.update_status(PlaybackStatus::Error { message: err_msg });
            return;
        }

        // Detect content type from headers and URL
        let mut hint = None;
        let mut mime = None;

        if let Some(ct) = response.headers().get(reqwest::header::CONTENT_TYPE) {
            if let Ok(ct_str) = ct.to_str() {
                let ct_lower = ct_str.to_lowercase();
                if ct_lower.contains("mpegurl") || ct_lower.contains("m3u") {
                    let err_msg = "HLS / M3U8 stream playlist format is not supported".to_string();
                    tracing::warn!("{err_msg}");
                    self.update_status(PlaybackStatus::Error { message: err_msg });
                    return;
                }
                if ct_lower.contains("aac") {
                    hint = Some("aac");
                    mime = Some("audio/aac");
                } else if ct_lower.contains("mpeg") || ct_lower.contains("mp3") {
                    hint = Some("mp3");
                    mime = Some("audio/mpeg");
                } else if ct_lower.contains("ogg")
                    || ct_lower.contains("vorbis")
                    || ct_lower.contains("opus")
                {
                    hint = Some("ogg");
                    mime = Some("audio/ogg");
                } else if ct_lower.contains("flac") {
                    hint = Some("flac");
                    mime = Some("audio/flac");
                }
            }
        }

        if hint.is_none() {
            let lower_url = url.to_lowercase();
            if lower_url.contains(".m3u8") || lower_url.contains(".m3u") {
                let err_msg = "HLS / M3U8 stream playlist format is not supported".to_string();
                tracing::warn!("{err_msg}");
                self.update_status(PlaybackStatus::Error { message: err_msg });
                return;
            }
            if lower_url.contains(".aac") || lower_url.contains("/aac") {
                hint = Some("aac");
                mime = Some("audio/aac");
            } else if lower_url.contains(".mp3") || lower_url.contains("/mp3") {
                hint = Some("mp3");
                mime = Some("audio/mpeg");
            } else if lower_url.contains(".ogg") || lower_url.contains("/ogg") {
                hint = Some("ogg");
                mime = Some("audio/ogg");
            } else if lower_url.contains(".flac") || lower_url.contains("/flac") {
                hint = Some("flac");
                mime = Some("audio/flac");
            }
        }

        // Prebuffer initial bytes (~48 KB) to allow format probing without underrun
        let mut prebuffer = Vec::new();
        while prebuffer.len() < 48 * 1024 {
            if stop_flag.load(Ordering::SeqCst) {
                return;
            }
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    prebuffer.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(e) => {
                    let err_msg = format!("Stream read error: {e}");
                    tracing::error!("{err_msg}");
                    self.update_status(PlaybackStatus::Error { message: err_msg });
                    return;
                }
            }
        }

        if prebuffer.is_empty() {
            let err_msg = "Stream returned empty audio data".to_string();
            tracing::error!("{err_msg}");
            self.update_status(PlaybackStatus::Error { message: err_msg });
            return;
        }

        // Check if stream is AAC (via headers, URL, or sniffing ADTS sync markers in prebuffer)
        let is_aac = hint == Some("aac") || is_adts_stream(&prebuffer);
        let adts_normalizer = if is_aac {
            hint = Some("aac");
            mime = Some("audio/aac");

            // Strip any partial frame or leading junk before the first valid ADTS frame
            if let Some(first_frame) = find_first_adts_frame(&prebuffer) {
                if first_frame > 0 {
                    prebuffer = prebuffer.split_off(first_frame);
                }
            }

            let mut normalizer = AdtsNormalizer::new();
            normalizer.process(&mut prebuffer);
            Some(normalizer)
        } else {
            None
        };

        // Bounded channel connects async network streamer to blocking Rodio mixer decoder
        let (tx_chunks, rx_chunks) = crossbeam_channel::bounded::<Vec<u8>>(128);
        let _ = tx_chunks.send(prebuffer);

        // Spawn background task to feed subsequent incoming stream chunks
        let stream_cancel = stop_flag.clone();
        tokio::spawn(async move {
            let mut normalizer = adts_normalizer;
            loop {
                if stream_cancel.load(Ordering::SeqCst) {
                    break;
                }
                match response.chunk().await {
                    Ok(Some(chunk)) => {
                        let mut data = chunk.to_vec();
                        if let Some(norm) = &mut normalizer {
                            norm.process(&mut data);
                        }
                        loop {
                            if stream_cancel.load(Ordering::SeqCst) {
                                break;
                            }
                            match tx_chunks.try_send(data) {
                                Ok(()) => break,
                                Err(crossbeam_channel::TrySendError::Full(pending)) => {
                                    data = pending;
                                    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                                }
                                Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                                    return; // Reader dropped or playback stopped
                                }
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        tracing::debug!("Audio stream chunk read ended: {e}");
                        break;
                    }
                }
            }
        });

        // Set probe limit to 64 KB so demuxer cannot scan infinite junk bytes if stream is invalid
        let is_probing = Arc::new(AtomicBool::new(true));
        let reader =
            ChannelReader::new(rx_chunks, stop_flag.clone(), is_probing.clone(), 64 * 1024);

        // Configure decoder builder with format hints
        let mut builder = rodio::Decoder::builder()
            .with_data(reader)
            .with_seekable(false);

        if let Some(h) = hint {
            builder = builder.with_hint(h);
        }
        if let Some(m) = mime {
            builder = builder.with_mime_type(m);
        }

        match builder.build() {
            Ok(decoder) => {
                // Probing succeeded! Disable the probe byte limit for continuous playback
                is_probing.store(false, Ordering::SeqCst);

                if let Some(player) = &self.player {
                    player.clear();
                    player.set_volume(self.current_volume);
                    player.append(decoder);
                    player.play();

                    self.update_status(PlaybackStatus::Playing {
                        url: url.clone(),
                        name: name.clone(),
                    });
                    tracing::info!("Now playing: '{}'", name);
                }
            }
            Err(e) => {
                let err_msg = format!("Audio decoder error: {e:?}");
                tracing::error!("{err_msg}");
                self.update_status(PlaybackStatus::Error { message: err_msg });
            }
        }
    }

    fn handle_stop(&mut self) {
        self.stop_current();
        self.update_status(PlaybackStatus::Stopped);
        tracing::info!("Playback stopped");
    }

    fn handle_pause(&mut self) {
        if let Some(player) = &self.player {
            player.pause();
        }
        if let PlaybackStatus::Playing { url, name } = &self.current_status {
            self.update_status(PlaybackStatus::Paused {
                url: url.clone(),
                name: name.clone(),
            });
        }
    }

    fn handle_resume(&mut self) {
        if let Some(player) = &self.player {
            player.play();
        }
        if let PlaybackStatus::Paused { url, name } = &self.current_status {
            self.update_status(PlaybackStatus::Playing {
                url: url.clone(),
                name: name.clone(),
            });
        }
    }

    fn handle_toggle_play_pause(&mut self) {
        match &self.current_status {
            PlaybackStatus::Playing { .. } => self.handle_pause(),
            PlaybackStatus::Paused { .. } => self.handle_resume(),
            _ => {}
        }
    }
}

pub fn playback_worker(
    rx_playback: crossbeam_channel::Receiver<PlaybackMessage>,
    proxy: EventLoopProxy<ProxyMessage>,
) {
    let mut worker = PlaybackWorker::new(proxy);

    let runtime = Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("playback_worker")
        .enable_all()
        .build()
        .expect("Failed to initialize Tokio runtime for PlaybackWorker");

    // Bridge crossbeam channel to tokio async channel
    let (async_tx, async_rx) = mpsc::unbounded_channel();
    let _ = std::thread::Builder::new()
        .name("playback_bridge".to_string())
        .spawn(move || {
            while let Ok(msg) = rx_playback.recv() {
                if async_tx.send(msg).is_err() {
                    break;
                }
            }
        });

    runtime.block_on(worker.run(async_rx));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_playback_status_transitions() {
        let status = PlaybackStatus::Stopped;
        assert_eq!(status, PlaybackStatus::Stopped);

        let loading = PlaybackStatus::Loading {
            url: "http://stream.org".to_string(),
            name: "Radio 1".to_string(),
        };
        assert_ne!(status, loading);
    }

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
