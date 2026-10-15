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

            // Current chunk exhausted, wait for next network chunk
            match self.rx.recv() {
                Ok(chunk) => {
                    self.current = Cursor::new(chunk);
                }
                Err(_) => {
                    // Channel closed (stream finished or aborted)
                    return Ok(0);
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

        // Bounded channel connects async network streamer to blocking Rodio mixer decoder
        let (tx_chunks, rx_chunks) = crossbeam_channel::bounded::<Vec<u8>>(128);
        let _ = tx_chunks.send(prebuffer);

        // Spawn background task to feed subsequent incoming stream chunks
        let stream_cancel = stop_flag.clone();
        tokio::spawn(async move {
            loop {
                if stream_cancel.load(Ordering::SeqCst) {
                    break;
                }
                match response.chunk().await {
                    Ok(Some(chunk)) => {
                        let mut data = chunk.to_vec();
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
}
