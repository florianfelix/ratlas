/// Helper to extract a direct stream URL from M3U or PLS playlist text.
pub fn parse_playlist_target(text: &str) -> Result<String, String> {
    if text.contains("#EXT-X-TARGETDURATION") || text.contains("#EXT-X-STREAM-INF") {
        return Err("HLS / M3U8 stream playlist format is not supported".to_string());
    }

    // Check PLS format: File1=http://...
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("File") {
            if let Some((_num, stream_url)) = rest.split_once('=') {
                let target = stream_url.trim();
                if target.starts_with("http://") || target.starts_with("https://") {
                    return Ok(target.to_string());
                }
            }
        }
    }

    // Check M3U format: lines starting with http:// or https:// (ignoring comments)
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            return Ok(trimmed.to_string());
        }
    }

    Err("Could not find playable audio stream in playlist".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_m3u_playlist() {
        let m3u = "#EXTM3U\n#EXTINF:-1,Radio Test\nhttp://stream.org/audio.mp3\n";
        assert_eq!(
            parse_playlist_target(m3u),
            Ok("http://stream.org/audio.mp3".to_string())
        );
    }

    #[test]
    fn test_parse_pls_playlist() {
        let pls =
            "[playlist]\nNumberOfEntries=1\nFile1=https://stream.org/audio.aac\nTitle1=Test\n";
        assert_eq!(
            parse_playlist_target(pls),
            Ok("https://stream.org/audio.aac".to_string())
        );
    }

    #[test]
    fn test_reject_hls_in_playlist() {
        let hls = "#EXTM3U\n#EXT-X-TARGETDURATION:4\nsegment1.ts\n";
        assert!(parse_playlist_target(hls).is_err());
    }
}
