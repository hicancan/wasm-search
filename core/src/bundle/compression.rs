use ruzstd::decoding::StreamingDecoder;
use ruzstd::encoding::{compress_to_vec, CompressionLevel};
use ruzstd::io::Read;

const MAX_DECODED_ARTIFACT_BYTES: usize = 64 * 1024 * 1024;

pub(super) fn compress_artifact(bytes: &[u8]) -> Vec<u8> {
    compress_to_vec(bytes, CompressionLevel::Fastest)
}

pub(crate) fn decompress_artifact(bytes: &[u8], expected_bytes: u64) -> Result<Vec<u8>, String> {
    let expected = usize::try_from(expected_bytes)
        .map_err(|_| "decoded artifact size does not fit this platform")?;
    if expected == 0 || expected > MAX_DECODED_ARTIFACT_BYTES {
        return Err(format!(
            "decoded artifact size {expected} is outside the supported range 1..={MAX_DECODED_ARTIFACT_BYTES}"
        ));
    }
    let limit = expected
        .checked_add(1)
        .ok_or("decoded artifact size overflow")?;
    let decoder = StreamingDecoder::new(bytes)
        .map_err(|error| format!("invalid zstd search artifact: {error:?}"))?;
    let mut decoded = Vec::with_capacity(expected);
    decoder
        .take(limit as u64)
        .read_to_end(&mut decoded)
        .map_err(|error| format!("failed to decompress search artifact: {error:?}"))?;
    if decoded.len() != expected {
        return Err(format!(
            "decoded search artifact size mismatch: expected {expected}, found {}",
            decoded.len()
        ));
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::{compress_artifact, decompress_artifact, MAX_DECODED_ARTIFACT_BYTES};

    #[test]
    fn round_trips_and_bounds_zstd_artifacts() {
        let input = b"njupt search artifact".repeat(512);
        let encoded = compress_artifact(&input);
        assert_eq!(
            decompress_artifact(&encoded, input.len() as u64).expect("decompress"),
            input
        );
        assert!(decompress_artifact(&encoded, (input.len() - 1) as u64).is_err());
        assert!(decompress_artifact(b"not zstd", 8).is_err());
        assert!(decompress_artifact(&[], 0).is_err());
        assert!(decompress_artifact(&[], (MAX_DECODED_ARTIFACT_BYTES + 1) as u64).is_err());
    }
}
