use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::runtime::Builder;
use tokio::sync::mpsc;
use winit::event_loop::EventLoopProxy;

use super::adts::{AdtsNormalizer, find_first_adts_frame, is_adts_stream};
use super::opus::OpusSource;
use super::playlist::parse_playlist_target;
use super::reader::ChannelReader;
use super::types::{PlaybackMessage, PlaybackStatus};
use crate::app::application::ProxyMessage;

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

        let mut stream_url = url.clone();
        let lower_initial = stream_url.to_lowercase();
        if lower_initial.contains(".m3u8") {
            let err_msg = "HLS / M3U8 stream playlist format is not supported".to_string();
            tracing::warn!("{err_msg}");
            self.update_status(PlaybackStatus::Error { message: err_msg });
            return;
        }

        // Connect to HTTP stream with redirect following and connect/read timeouts.
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .read_timeout(std::time::Duration::from_secs(15))
            .user_agent(crate::radio_api::APP_USER_AGENT)
            .build()
            .unwrap_or_default();

        // If URL explicitly points to a playlist file (.m3u, .pls), resolve to direct stream first
        if lower_initial.ends_with(".m3u") || lower_initial.ends_with(".pls") {
            tracing::info!("Resolving playlist URL: {}", stream_url);
            match client.get(&stream_url).send().await {
                Ok(resp) if resp.status().is_success() => match resp.text().await {
                    Ok(text) => match parse_playlist_target(&text) {
                        Ok(resolved) => {
                            tracing::info!("Resolved playlist '{}' -> '{}'", stream_url, resolved);
                            stream_url = resolved;
                        }
                        Err(e) => {
                            tracing::warn!("{e}");
                            self.update_status(PlaybackStatus::Error { message: e });
                            return;
                        }
                    },
                    Err(e) => {
                        let err_msg = format!("Failed to read playlist: {e}");
                        tracing::warn!("{err_msg}");
                        self.update_status(PlaybackStatus::Error { message: err_msg });
                        return;
                    }
                },
                Ok(resp) => {
                    let err_msg = format!("Playlist HTTP error: {}", resp.status());
                    tracing::warn!("{err_msg}");
                    self.update_status(PlaybackStatus::Error { message: err_msg });
                    return;
                }
                Err(e) => {
                    let err_msg = format!("Playlist connection failed: {e}");
                    tracing::warn!("{err_msg}");
                    self.update_status(PlaybackStatus::Error { message: err_msg });
                    return;
                }
            }
        }

        let mut response = match client
            .get(&stream_url)
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

        // Check if server returned a playlist Content-Type and resolve if needed
        if let Some(ct) = response.headers().get(reqwest::header::CONTENT_TYPE) {
            if let Ok(ct_str) = ct.to_str() {
                let ct_lower = ct_str.to_lowercase();
                if ct_lower.contains("mpegurl") || ct_lower.contains("scpls") {
                    if ct_lower.contains("m3u8") {
                        let err_msg =
                            "HLS / M3U8 stream playlist format is not supported".to_string();
                        tracing::warn!("{err_msg}");
                        self.update_status(PlaybackStatus::Error { message: err_msg });
                        return;
                    }
                    match response.text().await {
                        Ok(text) => match parse_playlist_target(&text) {
                            Ok(resolved) => {
                                tracing::info!("Resolved playlist content -> '{}'", resolved);
                                stream_url = resolved;
                                response = match client
                                    .get(&stream_url)
                                    .header("Icy-MetaData", "0")
                                    .header("Accept", "*/*")
                                    .send()
                                    .await
                                {
                                    Ok(resp) => resp,
                                    Err(e) => {
                                        let err_msg = format!("Stream connection failed: {e}");
                                        tracing::error!("{err_msg}");
                                        self.update_status(PlaybackStatus::Error {
                                            message: err_msg,
                                        });
                                        return;
                                    }
                                };
                            }
                            Err(e) => {
                                tracing::warn!("{e}");
                                self.update_status(PlaybackStatus::Error { message: e });
                                return;
                            }
                        },
                        Err(e) => {
                            let err_msg = format!("Failed to read playlist body: {e}");
                            tracing::warn!("{err_msg}");
                            self.update_status(PlaybackStatus::Error { message: err_msg });
                            return;
                        }
                    }
                }
            }
        }

        // Detect content type from headers and URL
        let mut hint = None;
        let mut mime = None;

        if let Some(ct) = response.headers().get(reqwest::header::CONTENT_TYPE) {
            if let Ok(ct_str) = ct.to_str() {
                let ct_lower = ct_str.to_lowercase();
                if ct_lower.contains("m3u8") {
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
            let lower_url = stream_url.to_lowercase();
            if lower_url.contains(".m3u8") {
                let err_msg = "HLS / M3U8 stream playlist format is not supported".to_string();
                tracing::warn!("{err_msg}");
                self.update_status(PlaybackStatus::Error { message: err_msg });
                return;
            }
            if lower_url.contains(".aac") || lower_url.contains("/aac") {
                hint = Some("aac");
                mime = Some("audio/aac");
            } else if lower_url.contains(".opus") || lower_url.contains("/opus") {
                hint = Some("opus");
                mime = Some("audio/opus");
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

        let is_opus = hint == Some("opus")
            || mime.is_some_and(|m| m.contains("opus"))
            || prebuffer.windows(8).any(|w| w == b"OpusHead");

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

        // If the stream is Opus, decode via our pure-Rust OpusSource
        if is_opus {
            match OpusSource::new(reader) {
                Ok(source) => {
                    is_probing.store(false, Ordering::SeqCst);

                    if let Some(player) = &self.player {
                        player.clear();
                        player.set_volume(self.current_volume);
                        player.append(source);
                        player.play();

                        self.update_status(PlaybackStatus::Playing {
                            url: url.clone(),
                            name: name.clone(),
                        });
                        tracing::info!("Now playing (Ogg Opus): '{}'", name);
                    }
                }
                Err(e) => {
                    let err_msg = format!("Opus decoder error: {e}");
                    tracing::error!("{err_msg}");
                    self.update_status(PlaybackStatus::Error { message: err_msg });
                }
            }
            return;
        }

        // Configure decoder builder with format hints for Rodio/Symphonia formats (Vorbis, MP3, AAC, FLAC)
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
                let err_msg = if is_opus {
                    "Ogg Opus format is not currently supported (only Ogg Vorbis, MP3, AAC, and FLAC are supported)".to_string()
                } else {
                    format!("Audio decoder error: {e:?}")
                };
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
