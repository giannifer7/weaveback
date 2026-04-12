// crates/weaveback-macro/src/lexer/mod.rs — generated from lexer.adoc
use crate::types::{LexerError, Token, TokenKind};
use memchr::{memchr, memmem};

#[cfg(test)]
mod tests;

fn is_identifier_start(b: u8) -> bool {
    matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'_')
}

fn is_identifier_continue(b: u8) -> bool {
    matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_')
}

fn is_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    /// Opening byte offset of the `%{` or `%name{`.
    Block(usize),
    /// Opening byte offset of the `%[` or `%name[`.
    Verbatim(usize),
    /// Opening byte offset of the `%name(`.
    Macro(usize),
    /// Comment state self-reports its own unclosed error.
    Comment,
}

/// What `handle_after_sigil` tells the caller to do.
#[derive(Debug, Clone, Copy, PartialEq)]
enum SpecialAction {
    /// A new state was pushed; return `true` to keep the current state active.
    Push,
    /// A block was closed; return `false` to pop the current state.
    Pop,
    /// A token was emitted; continue the loop.
    Continue,
}

pub struct Lexer<'a> {
    bytes: &'a [u8],
    pos: usize,
    src: u32,
    tokens: Vec<Token>,
    /// Configurable macro sigil (e.g. `%`, `@`, `§`).
    sigil: char,
    /// UTF-8 bytes of `sigil`, used by the byte-oriented scanner.
    sigil_bytes: Vec<u8>,
    state_stack: Vec<State>,
    pub errors: Vec<LexerError>,
    /// Precomputed `<sigil>/*` — checked in comment state.
    open_comment: Vec<u8>,
    /// Precomputed `<sigil>*/`.
    close_comment: Vec<u8>,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str, sigil: char, src: u32) -> Self {
        let sigil_bytes = sigil.to_string().into_bytes();
        let mut open_comment = sigil_bytes.clone();
        open_comment.extend_from_slice(b"/*");
        let mut close_comment = sigil_bytes.clone();
        close_comment.extend_from_slice(b"*/");
        let mut lexer = Lexer {
            bytes: input.as_bytes(),
            pos: 0,
            src,
            tokens: Vec::new(),
            sigil,
            sigil_bytes,
            state_stack: Vec::new(),
            errors: Vec::new(),
            open_comment,
            close_comment,
        };
        lexer.state_stack.push(State::Block(0));
        lexer
    }

    pub fn lex(mut self) -> (Vec<Token>, Vec<LexerError>) {
        self.run();
        (self.tokens, self.errors)
    }

    // ── Low-level cursor ──────────────────────────────────────────────────

    fn peek_byte(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<u8> {
        let b = self.bytes.get(self.pos).copied()?;
        self.pos += 1;
        Some(b)
    }

    fn starts_with_sigil(&self) -> bool {
        self.bytes[self.pos..].starts_with(&self.sigil_bytes)
    }

    fn advance_sigil(&mut self) -> bool {
        if self.starts_with_sigil() {
            self.pos += self.sigil_bytes.len();
            true
        } else {
            false
        }
    }

    /// Advance past the rest of the current line (through `\n` or to EOF).
    fn skip_line_comment(&mut self) {
        let rest = &self.bytes[self.pos..];
        match memchr(b'\n', rest) {
            Some(i) => self.pos += i + 1,
            None => self.pos = self.bytes.len(),
        }
    }

    /// Returns the byte index just past the end of an identifier starting at `start`.
    fn get_identifier_end(&self, start: usize) -> usize {
        let bytes = self.bytes;
        if start >= bytes.len() || !is_identifier_start(bytes[start]) {
            return start;
        }
        let mut end = start + 1;
        while end < bytes.len() && is_identifier_continue(bytes[end]) {
            end += 1;
        }
        end
    }

    fn starts_with_bytes(&self, pat: &[u8]) -> bool {
        self.bytes[self.pos..].starts_with(pat)
    }

    /// Extract the identifier tag from a `%tag{` or `%tag}` position.
    /// `pct_start` is the byte offset of `%`. Returns `""` for anonymous `%{`/`%}`.
    fn block_tag_at(&self, pct_start: usize) -> &str {
        let start = pct_start + self.sigil_bytes.len(); // skip sigil bytes
        let mut end = start;
        while end < self.bytes.len() && is_identifier_continue(self.bytes[end]) {
            end += 1;
        }
        std::str::from_utf8(&self.bytes[start..end]).unwrap_or("")
    }


    // ── Emission ──────────────────────────────────────────────────────────

    fn emit_token(&mut self, pos: usize, length: usize, kind: TokenKind) {
        if length == 0 && kind != TokenKind::EOF {
            return;
        }
        self.tokens.push(Token { kind, src: self.src, pos, length });
    }

    fn error_at(&mut self, pos: usize, message: &str) {
        self.errors.push(LexerError { pos, message: message.to_string() });
    }

    // ── Main driver ───────────────────────────────────────────────────────

    pub fn run(&mut self) {
        loop {
            // EOF is driven by input exhaustion, not by stack state.
            if self.pos >= self.bytes.len() {
                // Collect before borrowing &mut self for error_at.
                let unclosed: Vec<(String, usize)> = self.state_stack
                    .get(1..)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(|s| match s {
                        State::Block(p) => {
                            let tag = self.block_tag_at(*p);
                            let msg = if tag.is_empty() {
                                "Unclosed anonymous block '%{'".to_string()
                            } else {
                                format!("Unclosed block '%{}{{'", tag)
                            };
                            Some((msg, *p))
                        }
                        State::Verbatim(p) => {
                            let tag = self.block_tag_at(*p);
                            let msg = if tag.is_empty() {
                                "Unclosed anonymous block '%['".to_string()
                            } else {
                                format!("Unclosed block '%{}['", tag)
                            };
                            Some((msg, *p))
                        }
                        State::Macro(p) => {
                            Some(("Unclosed macro argument list".to_string(), *p))
                        }
                        State::Comment => None, // self-reported by run_comment_state
                    })
                    .collect();
                for (msg, pos) in unclosed {
                    self.error_at(pos, &msg);
                }
                self.emit_token(self.pos, 0, TokenKind::EOF);
                return;
            }
            let state = match self.state_stack.last().copied() {
                Some(s) => s,
                None => {
                    // Stack underflow before EOF — push/pop bug.
                    self.error_at(self.pos, "internal error: state stack underflow");
                    self.emit_token(self.pos, 0, TokenKind::EOF);
                    return;
                }
            };
            let keep_state = match state {
                State::Block(_) => self.run_block_state(),
                State::Verbatim(_) => self.run_verbatim_state(),
                State::Macro(_) => self.run_macro_state(),
                State::Comment => self.run_comment_state(),
            };
            if !keep_state {
                self.state_stack.pop();
            }
        }
    }

    // ── Block state ───────────────────────────────────────────────────────

    fn run_block_state(&mut self) -> bool {
        loop {
            let rest = &self.bytes[self.pos..];
            let text_len = match memmem::find(rest, &self.sigil_bytes) {
                Some(i) => i,
                None => {
                    if !rest.is_empty() {
                        self.emit_token(self.pos, rest.len(), TokenKind::Text);
                        self.pos = self.bytes.len();
                    }
                    return false;
                }
            };
            if text_len > 0 {
                self.emit_token(self.pos, text_len, TokenKind::Text);
                self.pos += text_len;
            }
            let pct_start = self.pos;
            self.advance_sigil(); // consume the sigil
            match self.handle_after_sigil(pct_start) {
                SpecialAction::Push => return true,
                SpecialAction::Pop => return false,
                SpecialAction::Continue => {}
            }
        }
    }

    // ── Verbatim state ────────────────────────────────────────────────────

    fn run_verbatim_state(&mut self) -> bool {
        loop {
            let rest = &self.bytes[self.pos..];
            let text_len = match memmem::find(rest, &self.sigil_bytes) {
                Some(i) => i,
                None => {
                    if !rest.is_empty() {
                        self.emit_token(self.pos, rest.len(), TokenKind::Text);
                        self.pos = self.bytes.len();
                    }
                    return false;
                }
            };
            if text_len > 0 {
                self.emit_token(self.pos, text_len, TokenKind::Text);
                self.pos += text_len;
            }

            let pct_start = self.pos;
            self.advance_sigil();

            if self.peek_byte() == Some(b'[') {
                self.advance();
                self.emit_token(pct_start, self.pos - pct_start, TokenKind::VerbatimOpen);
                self.state_stack.push(State::Verbatim(pct_start));
                return true;
            }

            if self.peek_byte() == Some(b']') {
                if self.state_stack.len() <= 1 {
                    self.error_at(pct_start, "Unmatched verbatim close: no open block");
                }
                self.advance();
                self.emit_token(pct_start, self.pos - pct_start, TokenKind::VerbatimClose);
                return false;
            }

            if self.peek_byte().is_some_and(is_identifier_start) {
                let id_end = self.get_identifier_end(self.pos);
                self.pos = id_end;
                match self.peek_byte() {
                    Some(b'[') => {
                        self.advance();
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::VerbatimOpen);
                        self.state_stack.push(State::Verbatim(pct_start));
                        return true;
                    }
                    Some(b']') => {
                        if self.state_stack.len() <= 1 {
                            self.error_at(pct_start, "Unmatched verbatim close: no open block");
                        }
                        self.advance();
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::VerbatimClose);
                        return false;
                    }
                    _ => {
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::Text);
                        continue;
                    }
                }
            }

            self.emit_token(pct_start, self.sigil_bytes.len(), TokenKind::Text);
        }
    }

    // ── Macro arg state ───────────────────────────────────────────────────

    fn run_macro_state(&mut self) -> bool {
        while let Some(b) = self.peek_byte() {
            if b == b')' {
                let start = self.pos;
                self.advance();
                self.emit_token(start, 1, TokenKind::CloseParen);
                return false;
            } else if b == b',' {
                let start = self.pos;
                self.advance();
                self.emit_token(start, 1, TokenKind::Comma);
            } else if b == b'=' {
                let start = self.pos;
                self.advance();
                self.emit_token(start, 1, TokenKind::Equal);
            } else if is_whitespace(b) {
                let ws_start = self.pos;
                while self.peek_byte().is_some_and(is_whitespace) {
                    self.advance();
                }
                self.emit_token(ws_start, self.pos - ws_start, TokenKind::Space);
            } else if self.starts_with_sigil() {
                let pct_start = self.pos;
                self.advance_sigil();
                match self.handle_after_sigil(pct_start) {
                    SpecialAction::Push => return true,
                    SpecialAction::Pop => return false,
                    SpecialAction::Continue => {}
                }
            } else if is_identifier_start(b) {
                let start = self.pos;
                let end = self.get_identifier_end(start);
                self.pos = end;
                self.emit_token(start, end - start, TokenKind::Ident);
            } else {
                    let start = self.pos;
                    while let Some(b2) = self.peek_byte() {
                        if is_whitespace(b2)
                            || matches!(b2, b')' | b',' | b'=')
                            || self.bytes[self.pos..].starts_with(&self.sigil_bytes)
                        {
                            break;
                        }
                    self.advance();
                }
                self.emit_token(start, self.pos - start, TokenKind::Text);
            }

            if !matches!(self.state_stack.last(), Some(State::Macro(_))) {
                return false;
            }
        }
        // EOF without closing ')'.
        if let Some(&State::Macro(open_pos)) = self.state_stack.last() {
            self.error_at(open_pos, "Unclosed macro argument list");
        }
        false
    }

    // ── Shared sigil-sequence handler ────────────────────────────────────
    //
    // Called after the sigil has been consumed.
    // `pct_start` is the byte offset of the sigil itself.

    fn handle_after_sigil(&mut self, pct_start: usize) -> SpecialAction {
        match self.peek_byte() {
            Some(b'(') => {
                self.handle_var(pct_start);
                SpecialAction::Continue
            }
            Some(b'{') => {
                self.advance();
                self.emit_token(pct_start, self.pos - pct_start, TokenKind::BlockOpen);
                self.state_stack.push(State::Block(pct_start));
                SpecialAction::Push
            }
            Some(b'}') => {
                if self.state_stack.len() <= 1 {
                    self.error_at(pct_start, "Unmatched block close: no open block");
                }
                self.advance();
                self.emit_token(pct_start, self.pos - pct_start, TokenKind::BlockClose);
                SpecialAction::Pop
            }
            Some(b'[') => {
                self.advance();
                self.emit_token(pct_start, self.pos - pct_start, TokenKind::VerbatimOpen);
                self.state_stack.push(State::Verbatim(pct_start));
                SpecialAction::Push
            }
            Some(b']') => {
                self.advance();
                self.emit_token(pct_start, self.pos - pct_start, TokenKind::VerbatimClose);
                self.error_at(pct_start, "Unmatched verbatim close outside verbatim block");
                SpecialAction::Continue
            }
            Some(b'/') => {
                self.advance();
                match self.peek_byte() {
                    Some(b'/') => {
                        self.advance();
                        self.skip_line_comment();
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::LineComment);
                    }
                    Some(b'*') => {
                        self.advance();
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::CommentOpen);
                        self.state_stack.push(State::Comment);
                        return SpecialAction::Push;
                    }
                    _ => {
                        self.error_at(
                            pct_start,
                            &format!(
                                "Unexpected char after '{}/': expected // or /*",
                                self.sigil
                            ),
                        );
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::Text);
                    }
                }
                SpecialAction::Continue
            }
            Some(b'-') => {
                self.advance();
                match self.peek_byte() {
                    Some(b'-') => {
                        self.advance();
                        self.skip_line_comment();
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::LineComment);
                    }
                    _ => {
                        self.error_at(
                            pct_start,
                            &format!(
                                "Unexpected char after '{}-': expected --",
                                self.sigil
                            ),
                        );
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::Text);
                    }
                }
                SpecialAction::Continue
            }
            Some(b'#') => {
                self.advance();
                self.skip_line_comment();
                self.emit_token(pct_start, self.pos - pct_start, TokenKind::LineComment);
                SpecialAction::Continue
            }
            Some(_) if self.starts_with_sigil() => {
                self.advance_sigil();
                self.emit_token(pct_start, self.pos - pct_start, TokenKind::Special);
                SpecialAction::Continue
            }
            Some(b) if is_identifier_start(b) => {
                let id_end = self.get_identifier_end(self.pos);
                self.pos = id_end;
                match self.peek_byte() {
                    Some(b'(') => {
                        self.advance();
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::Macro);
                        self.state_stack.push(State::Macro(pct_start));
                        SpecialAction::Push
                    }
                    Some(b'{') => {
                        self.advance();
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::BlockOpen);
                        self.state_stack.push(State::Block(pct_start));
                        SpecialAction::Push
                    }
                    Some(b'}') => {
                        if self.state_stack.len() <= 1 {
                            self.error_at(pct_start, "Unmatched block close: no open block");
                        }
                        self.advance();
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::BlockClose);
                        SpecialAction::Pop
                    }
                    Some(b'[') => {
                        self.advance();
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::VerbatimOpen);
                        self.state_stack.push(State::Verbatim(pct_start));
                        SpecialAction::Push
                    }
                    Some(b']') => {
                        self.advance();
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::VerbatimClose);
                        self.error_at(pct_start, "Unmatched verbatim close outside verbatim block");
                        SpecialAction::Continue
                    }
                    _ => {
                        // %identifier not followed by ( { } — pass through as plain text.
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::Text);
                        SpecialAction::Continue
                    }
                }
            }
            Some(_) => {
                // % followed by an unrecognized byte — emit just the % as Text with an error.
                // The unrecognized byte is left for the next iteration.
                self.error_at(
                    pct_start,
                    &format!("Unrecognized char after '{}'", self.sigil),
                );
                self.emit_token(pct_start, self.sigil_bytes.len(), TokenKind::Text);
                SpecialAction::Continue
            }
            None => {
                // % at EOF — emit as plain text.
                self.emit_token(pct_start, self.sigil_bytes.len(), TokenKind::Text);
                SpecialAction::Continue
            }
        }
    }

    /// Handle a `%(varname)` sequence. `pct_start` is the byte offset of the `%`.
    fn handle_var(&mut self, pct_start: usize) {
        self.advance(); // consume '('
        let ident_start = self.pos;
        let ident_end = self.get_identifier_end(ident_start);
        if ident_end > ident_start {
            self.pos = ident_end;
            if self.peek_byte() == Some(b')') {
                self.advance();
                self.emit_token(pct_start, self.pos - pct_start, TokenKind::Var);
                return;
            }
            self.error_at(pct_start, "Var missing closing ')'");
        } else {
            self.error_at(
                pct_start,
                &format!("Var missing identifier after '{}('", self.sigil),
            );
        }
        self.emit_token(pct_start, self.pos - pct_start, TokenKind::Text);
    }

    // ── Comment state ─────────────────────────────────────────────────────

    fn run_comment_state(&mut self) -> bool {
        let comment_text_start = self.pos;

        loop {
            // Jump to the next sigil — only it can start a delimiter.
            let rest = &self.bytes[self.pos..];
            let Some(i) = memmem::find(rest, &self.sigil_bytes) else {
                break; // EOF inside comment
            };
            self.pos += i;

            if self.starts_with_bytes(&self.open_comment) {
                if self.pos > comment_text_start {
                    self.emit_token(
                        comment_text_start,
                        self.pos - comment_text_start,
                        TokenKind::Text,
                    );
                }
                let delim_start = self.pos;
                self.pos += self.open_comment.len();
                self.emit_token(delim_start, self.open_comment.len(), TokenKind::CommentOpen);
                self.state_stack.push(State::Comment);
                return true;
            }
            if self.starts_with_bytes(&self.close_comment) {
                if self.pos > comment_text_start {
                    self.emit_token(
                        comment_text_start,
                        self.pos - comment_text_start,
                        TokenKind::Text,
                    );
                }
                let delim_start = self.pos;
                self.pos += self.close_comment.len();
                self.emit_token(delim_start, self.close_comment.len(), TokenKind::CommentClose);
                return false;
            }
            // Special char that isn't a comment delimiter — skip past it.
            self.pos += self.sigil_bytes.len();
        }

        // EOF: unclosed comment.
        if self.pos > comment_text_start {
            self.emit_token(
                comment_text_start,
                self.pos - comment_text_start,
                TokenKind::Text,
            );
        }
        self.error_at(comment_text_start, "Unclosed comment");
        false
    }
}
