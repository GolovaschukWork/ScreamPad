use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use sha2::{Digest, Sha256};

/// Read buffer for streaming hashing. Memory use stays constant regardless of file size.
const CHUNK_SIZE: usize = 64 * 1024;

/// Incremental SHA-256, for hashing bytes while they are being copied.
pub struct StreamHasher(Sha256);

impl StreamHasher {
    pub fn new() -> Self {
        Self(Sha256::new())
    }

    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    /// Lowercase hex digest (64 characters).
    pub fn finish(self) -> String {
        to_lower_hex(&self.0.finalize())
    }
}

impl Default for StreamHasher {
    fn default() -> Self {
        Self::new()
    }
}

/// SHA-256 of everything `reader` yields, as lowercase hex (64 characters). Reads in fixed-size chunks.
pub fn sha256_hex_reader(mut reader: impl Read) -> io::Result<String> {
    let mut hasher = StreamHasher::new();
    let mut buffer = vec![0_u8; CHUNK_SIZE];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => hasher.update(&buffer[..read]),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(hasher.finish())
}

/// SHA-256 of the file's bytes, streamed from disk.
pub fn sha256_hex_file(path: &Path) -> io::Result<String> {
    sha256_hex_reader(File::open(path)?)
}

fn to_lower_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    out
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn known_vectors() {
        assert_eq!(sha256_hex_reader(Cursor::new(b"")).expect("hash"), EMPTY);
        assert_eq!(sha256_hex_reader(Cursor::new(b"abc")).expect("hash"), ABC);
    }

    #[test]
    fn streaming_equals_one_shot_across_chunk_boundaries() {
        for size in [
            0,
            1,
            CHUNK_SIZE - 1,
            CHUNK_SIZE,
            CHUNK_SIZE + 1,
            3 * CHUNK_SIZE + 17,
        ] {
            let data: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
            let streamed = sha256_hex_reader(Cursor::new(&data)).expect("hash");
            assert_eq!(
                streamed,
                to_lower_hex(&Sha256::digest(&data)),
                "size {size}"
            );
        }
    }

    #[test]
    fn output_is_64_lowercase_hex_characters() {
        let hash = sha256_hex_reader(Cursor::new(b"anything")).expect("hash");
        assert_eq!(hash.len(), 64);
        assert!(
            hash.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
    }

    #[test]
    fn a_large_file_is_hashed_from_disk() {
        let path = std::env::temp_dir().join(format!("screampad-hash-{}.bin", std::process::id()));
        let chunk = vec![0x5a_u8; 1024 * 1024];
        {
            use std::io::Write;
            let mut file = File::create(&path).expect("create");
            for _ in 0..8 {
                file.write_all(&chunk).expect("write");
            }
        }
        let from_disk = sha256_hex_file(&path).expect("hash file");
        let _ = std::fs::remove_file(&path);
        let mut expected = Sha256::new();
        for _ in 0..8 {
            expected.update(&chunk);
        }
        assert_eq!(from_disk, to_lower_hex(&expected.finalize()));
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_panic() {
        let path = std::env::temp_dir().join("screampad-definitely-missing-file.bin");
        assert!(sha256_hex_file(&path).is_err());
    }

    struct FailingReader;
    impl Read for FailingReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("disk error"))
        }
    }

    #[test]
    fn read_errors_are_propagated() {
        assert!(sha256_hex_reader(FailingReader).is_err());
    }
}
