//! WAV 文件辅助：PCM → WAV 头，分段（智谱 30 秒限制）。

use anyhow::Result;

pub const SAMPLE_RATE: u32 = 16000;
pub const BITS_PER_SAMPLE: u16 = 16;
pub const CHANNELS: u16 = 1;

/// 用 PCM16 mono 16kHz 构造 WAV 文件。
pub fn build_wav(pcm: &[u8]) -> Vec<u8> {
    let byte_rate = SAMPLE_RATE * CHANNELS as u32 * (BITS_PER_SAMPLE as u32 / 8);
    let block_align = CHANNELS * (BITS_PER_SAMPLE / 8);
    let data_size = pcm.len() as u32;
    let mut buf = Vec::with_capacity(44 + pcm.len());
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_size).to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // PCM chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    buf.extend_from_slice(&CHANNELS.to_le_bytes());
    buf.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());
    buf.extend_from_slice(pcm);
    buf
}

/// 按时长分段。若音频短于 max_seconds，返回单元素切片。
/// 否则按 PCM 字节对齐切分。
pub fn split_wav(wav: &[u8], max_seconds: f64) -> Result<Vec<Vec<u8>>> {
    if wav.len() < 44 {
        return Ok(Vec::new());
    }
    let sample_rate = u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]);
    let bits_per_sample = u16::from_le_bytes([wav[34], wav[35]]);
    if bits_per_sample == 0 || sample_rate == 0 {
        return Ok(vec![wav.to_vec()]);
    }
    let bytes_per_sec = (sample_rate as u64) * (CHANNELS as u64) * (bits_per_sample as u64 / 8);
    let pcm = &wav[44..];
    let total_secs = pcm.len() as f64 / bytes_per_sec as f64;
    if total_secs <= max_seconds {
        return Ok(vec![wav.to_vec()]);
    }
    let chunk_bytes = (bytes_per_sec as f64 * max_seconds) as usize;
    let chunk_bytes = chunk_bytes - (chunk_bytes % (bits_per_sample as usize / 8));
    let mut chunks = Vec::new();
    let mut off = 0;
    while off < pcm.len() {
        let end = (off + chunk_bytes).min(pcm.len());
        chunks.push(build_wav(&pcm[off..end]));
        off = end;
    }
    Ok(chunks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_44_bytes() {
        let wav = build_wav(&[]);
        assert_eq!(wav.len(), 44);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
    }

    #[test]
    fn split_short_returns_one() {
        let pcm = vec![0u8; 1000];
        let wav = build_wav(&pcm);
        let parts = split_wav(&wav, 30.0).unwrap();
        assert_eq!(parts.len(), 1);
    }

    #[test]
    fn split_long_multiple() {
        // 40 秒 16kHz 16bit mono = 40*16000*2 = 1,280,000 bytes
        let pcm = vec![0u8; 40 * 16000 * 2];
        let wav = build_wav(&pcm);
        let parts = split_wav(&wav, 30.0).unwrap();
        assert!(parts.len() >= 2);
    }
}
