// weaveback-serve/src/server/ai/stream.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

pub(crate) struct AiChannelReader {
    rx:  std::sync::mpsc::Receiver<String>,
    buf: Vec<u8>,
    pos: usize,
}

impl AiChannelReader {
    pub(crate) fn new(rx: std::sync::mpsc::Receiver<String>) -> Self {
        // Prime with a keepalive comment so EventSource confirms the connection.
        Self { rx, buf: b": weaveback-ai\n\n".to_vec(), pos: 0 }
    }
}

impl Read for AiChannelReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        loop {
            if self.pos < self.buf.len() {
                let n = out.len().min(self.buf.len() - self.pos);
                out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
                self.pos += n;
                return Ok(n);
            }
            match self.rx.recv() {
                Ok(s) => { self.buf = s.into_bytes(); self.pos = 0; }
                Err(_) => return Ok(0),
            }
        }
    }
}

pub(crate) fn sse_headers() -> Vec<Header> {
    vec![
        Header::from_bytes("Content-Type",               "text/event-stream").unwrap(),
        Header::from_bytes("Cache-Control",              "no-cache").unwrap(),
        Header::from_bytes("X-Accel-Buffering",          "no").unwrap(),
        Header::from_bytes("Access-Control-Allow-Origin","*").unwrap(),
    ]
}

