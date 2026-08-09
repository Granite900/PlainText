//! Bundling a program into a copy of the interpreter, for `plaintext build`.
//!
//! The interpreter binary already contains everything needed to run any
//! program — the language, the stdlib, Raylib. So "compiling" a `.pt` file is
//! not code generation: it's copying that binary and **appending the program's
//! source** to it, with a small footer the binary looks for at startup:
//!
//! ```text
//! [ runtime binary ][ payload ][ payload_len: u64 LE ][ MAGIC (8 bytes) ]
//! ```
//!
//! On startup the binary reads its own tail; if the magic is there it runs the
//! embedded program instead of acting as the CLI. Because appending bytes is
//! platform-agnostic, a Windows machine can bundle a program into a *macOS*
//! runtime binary just as easily as its own — no cross-compiler needed.

use std::io::{Read, Seek, SeekFrom};

const MAGIC: &[u8; 8] = b"PTBUNDL1";
const FOOTER: usize = 8 /* payload_len */ + 8 /* magic */;

/// An embedded program: the entry file's key plus every source file by key.
pub struct Payload {
    pub entry: String,
    pub files: Vec<(String, String)>,
}

/// Serialize a payload (see the module docs for the layout).
pub fn encode(payload: &Payload) -> Vec<u8> {
    let mut out = Vec::new();
    put_str(&mut out, &payload.entry);
    put_u32(&mut out, payload.files.len() as u32);
    for (key, src) in &payload.files {
        put_str(&mut out, key);
        put_str(&mut out, src);
    }
    out
}

/// Read back what `encode` wrote. `None` if the bytes are malformed.
pub fn decode(bytes: &[u8]) -> Option<Payload> {
    let mut c = Reader { bytes, pos: 0 };
    let entry = c.take_str()?;
    let n = c.take_u32()? as usize;
    let mut files = Vec::with_capacity(n);
    for _ in 0..n {
        let key = c.take_str()?;
        let src = c.take_str()?;
        files.push((key, src));
    }
    Some(Payload { entry, files })
}

/// Append a payload to a runtime binary, producing the bundled executable.
pub fn append(runtime: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(runtime.len() + payload.len() + FOOTER);
    out.extend_from_slice(runtime);
    out.extend_from_slice(payload);
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(MAGIC);
    out
}

/// The runtime portion of `exe`, without any appended payload — so re-bundling
/// an already-bundled binary starts from the clean runtime.
pub fn strip(exe: &[u8]) -> &[u8] {
    match payload_range(exe) {
        Some((start, _)) => &exe[..start],
        None => exe,
    }
}

/// Extract a payload from a whole executable image (used by `--run`/tests).
pub fn extract(exe: &[u8]) -> Option<Payload> {
    let (start, end) = payload_range(exe)?;
    decode(&exe[start..end])
}

/// Read the payload embedded in the currently-running executable, if any. Reads
/// only the tail, so it's cheap when there's no payload (the normal CLI case).
pub fn read_self() -> Option<Payload> {
    let path = std::env::current_exe().ok()?;
    let mut f = std::fs::File::open(path).ok()?;
    let size = f.metadata().ok()?.len();
    if size < FOOTER as u64 {
        return None;
    }
    f.seek(SeekFrom::Start(size - FOOTER as u64)).ok()?;
    let mut footer = [0u8; FOOTER];
    f.read_exact(&mut footer).ok()?;
    if &footer[8..] != MAGIC {
        return None;
    }
    let len = u64::from_le_bytes(footer[..8].try_into().ok()?);
    if len + FOOTER as u64 > size {
        return None;
    }
    f.seek(SeekFrom::Start(size - FOOTER as u64 - len)).ok()?;
    let mut buf = vec![0u8; len as usize];
    f.read_exact(&mut buf).ok()?;
    decode(&buf)
}

/// If `exe` ends with our footer, the `[start, end)` byte range of its payload.
fn payload_range(exe: &[u8]) -> Option<(usize, usize)> {
    if exe.len() < FOOTER {
        return None;
    }
    let (body, footer) = exe.split_at(exe.len() - FOOTER);
    if &footer[8..] != MAGIC {
        return None;
    }
    let len = u64::from_le_bytes(footer[..8].try_into().ok()?) as usize;
    if len > body.len() {
        return None;
    }
    Some((body.len() - len, body.len()))
}

// ---- tiny length-prefixed encoding (no external crates) ------------------

fn put_u32(out: &mut Vec<u8>, n: u32) {
    out.extend_from_slice(&n.to_le_bytes());
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    put_u32(out, s.len() as u32);
    out.extend_from_slice(s.as_bytes());
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take_u32(&mut self) -> Option<u32> {
        let slice = self.bytes.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some(u32::from_le_bytes(slice.try_into().ok()?))
    }

    fn take_str(&mut self) -> Option<String> {
        let n = self.take_u32()? as usize;
        let slice = self.bytes.get(self.pos..self.pos + n)?;
        self.pos += n;
        String::from_utf8(slice.to_vec()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Payload {
        Payload {
            entry: "main.pt".into(),
            files: vec![
                ("main.pt".into(), "import \"./lib.pt\"\nprint(hi())".into()),
                ("lib.pt".into(), "make function called hi() { return \"hi\" }".into()),
            ],
        }
    }

    #[test]
    fn encode_decode_round_trip() {
        let p = sample();
        let back = decode(&encode(&p)).expect("decodes");
        assert_eq!(back.entry, p.entry);
        assert_eq!(back.files, p.files);
    }

    #[test]
    fn append_strip_extract_round_trip() {
        let runtime = b"PRETEND-INTERPRETER-BINARY".to_vec();
        let image = append(&runtime, &encode(&sample()));
        assert_eq!(strip(&image), &runtime[..]);
        let back = extract(&image).expect("extracts");
        assert_eq!(back.files.len(), 2);
        // A plain binary with no footer has nothing to extract.
        assert!(extract(&runtime).is_none());
    }
}
