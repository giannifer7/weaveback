// weaveback-serve/src/server/sse.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

pub(crate) struct SseReader {
    rx: std::sync::mpsc::Receiver<()>,
    buf: Vec<u8>,
    pos: usize,
}

impl SseReader {
    pub(crate) fn new(rx: std::sync::mpsc::Receiver<()>) -> Self {
        // Prime the buffer with a keepalive comment so the SSE connection is
        // established immediately.
        Self {
            rx,
            buf: b": weaveback-serve\n\n".to_vec(),
            pos: 0,
        }
    }
}

impl Read for SseReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        loop {
            if self.pos < self.buf.len() {
                let n = out.len().min(self.buf.len() - self.pos);
                out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
                self.pos += n;
                return Ok(n);
            }
            // Buffer exhausted — wait for the next reload signal.
            match self.rx.recv() {
                Ok(()) => {
                    self.buf = b"event: reload\ndata:\n\n".to_vec();
                    self.pos = 0;
                }
                Err(_) => return Ok(0), // sender dropped → EOF
            }
        }
    }
}

