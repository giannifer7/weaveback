// crates/weaveback-macro/src/lexer/mod.rs

use crate::types::{LexerError, Token, TokenKind};
use memchr::memchr;

#[cfg(test)]
mod tests;

fn is_identifier_start(c: char) -> bool {
    matches!(c, 'a'..='z' | 'A'..='Z' | '_')
}

fn is_identifier_continue(c: char) -> bool {
    matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_')
}

fn is_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Block,
    Macro,
    Comment,
}

/// What `handle_after_special` tells the caller to do.
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
    input: &'a str,
    pos: usize,
    src: u32,
    tokens: Vec<Token>,
    special_char: char,
    state_stack: Vec<State>,
    pub errors: Vec<LexerError>,
    /// Precomputed `{special}/*` string — checked once per char in comment state.
    open_comment: String,
    /// Precomputed `{special}*/` string.
    close_comment: String,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str, special_char: char, src: u32) -> Self {
        let mut lexer = Lexer {
            input,
            pos: 0,
            src,
            tokens: Vec::new(),
            special_char,
            state_stack: Vec::new(),
            errors: Vec::new(),
            open_comment: format!("{}/*", special_char),
            close_comment: format!("{}*/", special_char),
        };
        lexer.state_stack.push(State::Block);
        lexer
    }

    pub fn lex(mut self) -> (Vec<Token>, Vec<LexerError>) {
        self.run();
        (self.tokens, self.errors)
    }

    // -------------------------------------------------------------------------
    // Low-level input helpers
    // -------------------------------------------------------------------------

    fn peek_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.input[self.pos..].chars().next()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    /// Advance until `end_char` is consumed (or EOF).
    fn read_until(&mut self, end_char: char) {
        while let Some(ch) = self.advance() {
            if ch == end_char {
                break;
            }
        }
    }

    /// Returns the byte index just past the end of an identifier starting at `start`.
    fn get_identifier_end(&self, start: usize) -> usize {
        let mut end = start;
        let mut chars = self.input[start..].chars();
        if let Some(c) = chars.next() {
            if !is_identifier_start(c) {
                return end;
            }
            end += c.len_utf8();
        }
        for c in chars {
            if !is_identifier_continue(c) {
                break;
            }
            end += c.len_utf8();
        }
        end
    }

    fn starts_with(&self, s: &str) -> bool {
        self.input[self.pos..].starts_with(s)
    }

    // -------------------------------------------------------------------------
    // Token / error emission
    // -------------------------------------------------------------------------

    fn emit_token(&mut self, pos: usize, length: usize, kind: TokenKind) {
        if length == 0 && kind != TokenKind::EOF {
            return;
        }
        self.tokens.push(Token { kind, src: self.src, pos, length });
    }

    fn error_at(&mut self, pos: usize, message: &str) {
        self.errors.push(LexerError { pos, message: message.to_string() });
    }

    // -------------------------------------------------------------------------
    // Main driver
    // -------------------------------------------------------------------------

    pub fn run(&mut self) {
        loop {
            if self.state_stack.is_empty() {
                self.emit_token(self.pos, 0, TokenKind::EOF);
                return;
            }
            let keep_state = match self.state_stack.last().copied().unwrap() {
                State::Block => self.run_block_state(),
                State::Macro => self.run_macro_state(),
                State::Comment => self.run_comment_state(),
            };
            if !keep_state {
                self.state_stack.pop();
            }
        }
    }

    // -------------------------------------------------------------------------
    // BLOCK STATE
    // -------------------------------------------------------------------------

    fn run_block_state(&mut self) -> bool {
        let sc = self.special_char as u8;
        loop {
            let rest = self.input.as_bytes()[self.pos..].as_ref();
            let text_len = match memchr(sc, rest) {
                Some(i) => i,
                None => {
                    if !rest.is_empty() {
                        self.emit_token(self.pos, rest.len(), TokenKind::Text);
                        self.pos = self.input.len();
                    }
                    return false;
                }
            };
            if text_len > 0 {
                self.emit_token(self.pos, text_len, TokenKind::Text);
                self.pos += text_len;
            }
            let pct_start = self.pos;
            self.advance(); // consume the special char
            match self.handle_after_special(pct_start) {
                SpecialAction::Push => return true,
                SpecialAction::Pop => return false,
                SpecialAction::Continue => {}
            }
        }
    }

    // -------------------------------------------------------------------------
    // MACRO STATE
    // -------------------------------------------------------------------------

    fn run_macro_state(&mut self) -> bool {
        while let Some(ch) = self.peek_char() {
            if ch == ')' {
                let start = self.pos;
                self.advance();
                self.emit_token(start, 1, TokenKind::CloseParen);
                return false;
            } else if ch == ',' {
                let start = self.pos;
                self.advance();
                self.emit_token(start, 1, TokenKind::Comma);
            } else if ch == '=' {
                let start = self.pos;
                self.advance();
                self.emit_token(start, 1, TokenKind::Equal);
            } else if is_whitespace(ch) {
                let ws_start = self.pos;
                while self.peek_char().map_or(false, is_whitespace) {
                    self.advance();
                }
                self.emit_token(ws_start, self.pos - ws_start, TokenKind::Space);
            } else if ch == self.special_char {
                let pct_start = self.pos;
                self.advance();
                match self.handle_after_special(pct_start) {
                    SpecialAction::Push => return true,
                    SpecialAction::Pop => return false,
                    SpecialAction::Continue => {}
                }
            } else if is_identifier_start(ch) {
                let start = self.pos;
                let end = self.get_identifier_end(start);
                self.pos = end;
                self.emit_token(start, end - start, TokenKind::Ident);
            } else {
                let start = self.pos;
                while let Some(ch2) = self.peek_char() {
                    if is_whitespace(ch2)
                        || matches!(ch2, ')' | ',' | '=')
                        || ch2 == self.special_char
                    {
                        break;
                    }
                    self.advance();
                }
                self.emit_token(start, self.pos - start, TokenKind::Text);
            }

            if self.state_stack.last() != Some(&State::Macro) {
                return false;
            }
        }
        false
    }

    // -------------------------------------------------------------------------
    // Shared special-sequence handler
    //
    // Called after the special char has been consumed.
    // `pct_start` is the byte offset of the special char itself.
    // -------------------------------------------------------------------------

    fn handle_after_special(&mut self, pct_start: usize) -> SpecialAction {
        match self.peek_char() {
            Some('(') => {
                self.handle_var(pct_start);
                SpecialAction::Continue
            }
            Some('{') => {
                self.advance();
                self.emit_token(pct_start, self.pos - pct_start, TokenKind::BlockOpen);
                self.state_stack.push(State::Block);
                SpecialAction::Push
            }
            Some('}') => {
                if self.state_stack.len() <= 1 {
                    self.error_at(pct_start, "Unmatched block close: no open block");
                }
                self.advance();
                self.emit_token(pct_start, self.pos - pct_start, TokenKind::BlockClose);
                SpecialAction::Pop
            }
            Some('/') => {
                self.advance();
                match self.peek_char() {
                    Some('/') => {
                        self.advance();
                        self.read_until('\n');
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::LineComment);
                    }
                    Some('*') => {
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
                                self.special_char
                            ),
                        );
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::Text);
                    }
                }
                SpecialAction::Continue
            }
            Some('-') => {
                self.advance();
                match self.peek_char() {
                    Some('-') => {
                        self.advance();
                        self.read_until('\n');
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::LineComment);
                    }
                    _ => {
                        self.error_at(
                            pct_start,
                            &format!(
                                "Unexpected char after '{}-': expected --",
                                self.special_char
                            ),
                        );
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::Text);
                    }
                }
                SpecialAction::Continue
            }
            Some('#') => {
                self.advance();
                self.read_until('\n');
                self.emit_token(pct_start, self.pos - pct_start, TokenKind::LineComment);
                SpecialAction::Continue
            }
            Some(c) if c == self.special_char => {
                self.advance();
                self.emit_token(pct_start, self.pos - pct_start, TokenKind::Special);
                SpecialAction::Continue
            }
            Some(c) if is_identifier_start(c) => {
                let id_end = self.get_identifier_end(self.pos);
                self.pos = id_end;
                match self.peek_char() {
                    Some('(') => {
                        self.advance();
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::Macro);
                        self.state_stack.push(State::Macro);
                        SpecialAction::Push
                    }
                    Some('{') => {
                        self.advance();
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::BlockOpen);
                        self.state_stack.push(State::Block);
                        SpecialAction::Push
                    }
                    Some('}') => {
                        if self.state_stack.len() <= 1 {
                            self.error_at(pct_start, "Unmatched block close: no open block");
                        }
                        self.advance();
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::BlockClose);
                        SpecialAction::Pop
                    }
                    _ => {
                        // %identifier not followed by ( { } — pass through as plain text.
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::Text);
                        SpecialAction::Continue
                    }
                }
            }
            Some(_) => {
                // % followed by an unrecognized char — emit just the % as Text with an error.
                // The unrecognized char is left for the next iteration.
                self.error_at(
                    pct_start,
                    &format!("Unrecognized char after '{}'", self.special_char),
                );
                self.emit_token(pct_start, self.special_char.len_utf8(), TokenKind::Text);
                SpecialAction::Continue
            }
            None => {
                // % at EOF — emit as plain text.
                self.emit_token(pct_start, self.special_char.len_utf8(), TokenKind::Text);
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
            if self.peek_char() == Some(')') {
                self.advance();
                self.emit_token(pct_start, self.pos - pct_start, TokenKind::Var);
                return;
            }
            self.error_at(pct_start, "Var missing closing ')'");
        } else {
            self.error_at(
                pct_start,
                &format!("Var missing identifier after '{}('", self.special_char),
            );
        }
        self.emit_token(pct_start, self.pos - pct_start, TokenKind::Text);
    }

    // -------------------------------------------------------------------------
    // COMMENT STATE
    // -------------------------------------------------------------------------

    fn run_comment_state(&mut self) -> bool {
        let sc = self.special_char as u8;
        let open_len = self.open_comment.len();
        let close_len = self.close_comment.len();
        let comment_text_start = self.pos;

        loop {
            // Jump to the next special char — only it can start a delimiter.
            let rest = self.input.as_bytes()[self.pos..].as_ref();
            let Some(i) = memchr(sc, rest) else {
                break; // EOF inside comment
            };
            self.pos += i;

            if self.starts_with(&self.open_comment) {
                if self.pos > comment_text_start {
                    self.emit_token(
                        comment_text_start,
                        self.pos - comment_text_start,
                        TokenKind::Text,
                    );
                }
                let delim_start = self.pos;
                self.pos += open_len;
                self.emit_token(delim_start, open_len, TokenKind::CommentOpen);
                self.state_stack.push(State::Comment);
                return true;
            }
            if self.starts_with(&self.close_comment) {
                if self.pos > comment_text_start {
                    self.emit_token(
                        comment_text_start,
                        self.pos - comment_text_start,
                        TokenKind::Text,
                    );
                }
                let delim_start = self.pos;
                self.pos += close_len;
                self.emit_token(delim_start, close_len, TokenKind::CommentClose);
                return false;
            }
            // Special char that isn't a comment delimiter — skip it and continue.
            self.advance();
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
