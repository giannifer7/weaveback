// crates/weaveback-macro/src/lexer/mod.rs

use crate::types::{LexerError, Token, TokenKind};

#[cfg(test)]
mod tests;

/// Returns true if `c` is an ASCII letter or underscore.
fn is_identifier_start(c: char) -> bool {
    matches!(c, 'a'..='z' | 'A'..='Z' | '_')
}

/// Returns true if `c` is an ASCII letter, digit or underscore.
fn is_identifier_continue(c: char) -> bool {
    matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_')
}

/// Returns true if `c` is a whitespace character.
fn is_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

/// The states in which the lexer can operate.
#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Block,
    Macro,
    Comment,
}

/// The lexer struct.
pub struct Lexer<'a> {
    input: &'a str,
    bytes: &'a [u8],
    pos: usize,
    line: usize,
    column: usize,
    src: u32,
    tokens: Vec<Token>,
    special_char: char,
    state_stack: Vec<State>,
    pub errors: Vec<LexerError>,
    last_line: usize,
    last_col: usize,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer.
    pub fn new(input: &'a str, special_char: char, src: u32) -> Self {
        let bytes = input.as_bytes();
        let mut lexer = Lexer {
            input,
            bytes,
            pos: 0,
            line: 1,
            column: 1,
            src,
            tokens: Vec::new(),
            special_char,
            state_stack: Vec::new(),
            errors: Vec::new(),
            last_line: 1,
            last_col: 1,
        };
        lexer.state_stack.push(State::Block);
        lexer
    }

    /// Run the lexer and return the collected tokens.
    pub fn lex(mut self) -> (Vec<Token>, Vec<LexerError>) {
        self.run();
        (self.tokens, self.errors)
    }

    /// Record the current line/column (used to silence warnings about unused variables).
    fn record_position(&mut self, line: usize, col: usize) {
        self.last_line = line;
        self.last_col = col;
    }

    /// Peek at the current character (with its line/column), without advancing.
    fn peek_char_and_pos(&self) -> (Option<char>, usize, usize) {
        if self.pos >= self.bytes.len() {
            (None, self.line, self.column)
        } else {
            let c = self.input[self.pos..].chars().next().unwrap();
            (Some(c), self.line, self.column)
        }
    }

    /// Advance one character, returning the consumed character along with its original (line, col).
    fn advance(&mut self) -> Option<(char, usize, usize)> {
        let (ch_opt, old_line, old_col) = self.peek_char_and_pos();
        if let Some(ch) = ch_opt {
            self.pos += ch.len_utf8();
            if ch == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
            Some((ch, old_line, old_col))
        } else {
            None
        }
    }

    /// Read until the specified `end_char` or until EOF.
    fn read_until(&mut self, end_char: char) {
        while let (Some(ch), _, _) = self.peek_char_and_pos() {
            self.advance();
            if ch == end_char {
                break;
            }
        }
    }

    /// Returns the byte index just after the end of an identifier beginning at `start`.
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

    /// Helper: check if the input at the current position starts with string `s`.
    fn starts_with(&self, s: &str) -> bool {
        self.input[self.pos..].starts_with(s)
    }

    /// Helper: if the input at the current position starts with string `s`,
    /// consume it (updating `pos`, `line`, and `column`) and return true.
    fn consume_str(&mut self, s: &str) -> bool {
        if self.starts_with(s) {
            for ch in s.chars() {
                if ch == '\n' {
                    self.line += 1;
                    self.column = 1;
                } else {
                    self.column += 1;
                }
            }
            self.pos += s.len();
            true
        } else {
            false
        }
    }

    /// Emit a token (unless length is zero and the kind is not EOF).
    fn emit_token(&mut self, pos: usize, length: usize, kind: TokenKind) {
        if length == 0 && kind != TokenKind::EOF {
            return;
        }
        self.tokens.push(Token {
            kind,
            src: self.src,
            pos,
            length,
        });
    }

    /// Record an error at the given (row, col) with the specified message.
    fn error_here(&mut self, row: usize, col: usize, message: &str) {
        self.errors.push(LexerError {
            row,
            col,
            message: message.to_string(),
        });
    }

    /// Main lexer driver.
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

    //--------------------------------------------------------------------------
    // BLOCK STATE
    //--------------------------------------------------------------------------
    fn run_block_state(&mut self) -> bool {
        let mut text_start = self.pos;

        while let (Some(ch), line, col) = self.peek_char_and_pos() {
            self.record_position(line, col);

            if ch == self.special_char {
                // Flush any accumulated text.
                if self.pos > text_start {
                    self.emit_token(text_start, self.pos - text_start, TokenKind::Text);
                }
                let (pct_char, pct_line, pct_col) = self.advance().unwrap();
                self.record_position(pct_line, pct_col);

                let pct_start = self.pos - pct_char.len_utf8();
                let (next_opt, nxt_line, nxt_col) = self.peek_char_and_pos();
                if let Some(nch) = next_opt {
                    if nch == '(' {
                        // Handle a variable.
                        self.handle_var(pct_start, pct_line, pct_col);
                    } else if nch == '{' {
                        self.advance();
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::BlockOpen);
                        self.state_stack.push(State::Block);
                        return true;
                    } else if nch == '}' {
                        if self.state_stack.len() <= 1 {
                            self.error_here(
                                nxt_line,
                                nxt_col.saturating_sub(1),
                                "Unmatched block close: no open block",
                            );
                        }
                        self.advance();
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::BlockClose);
                        return false;
                    } else if nch == '/' {
                        self.advance();
                        if let (Some(c2), c2_line, c2_col) = self.peek_char_and_pos() {
                            self.record_position(c2_line, c2_col);
                            if c2 == '/' {
                                self.advance();
                                self.read_until('\n');
                                self.emit_token(
                                    pct_start,
                                    self.pos - pct_start,
                                    TokenKind::LineComment,
                                );
                            } else if c2 == '*' {
                                self.advance();
                                self.emit_token(
                                    pct_start,
                                    self.pos - pct_start,
                                    TokenKind::CommentOpen,
                                );
                                self.state_stack.push(State::Comment);
                                return true;
                            } else {
                                self.error_here(
                                    c2_line,
                                    c2_col,
                                    &format!(
                                        "Unexpected char after '{}{}' in block",
                                        self.special_char, "/"
                                    ),
                                );
                                self.emit_token(pct_start, self.pos - pct_start, TokenKind::Text);
                            }
                        }
                    } else if nch == '-' {
                        self.advance();
                        if let (Some(d), d_line, d_col) = self.peek_char_and_pos() {
                            self.record_position(d_line, d_col);
                            if d == '-' {
                                self.advance();
                                self.read_until('\n');
                                self.emit_token(
                                    pct_start,
                                    self.pos - pct_start,
                                    TokenKind::LineComment,
                                );
                            } else {
                                self.error_here(
                                    d_line,
                                    d_col,
                                    &format!(
                                        "Unexpected char after '{}{}' in block",
                                        self.special_char, "-"
                                    ),
                                );
                                self.emit_token(pct_start, self.pos - pct_start, TokenKind::Text);
                            }
                        }
                    } else if nch == '#' {
                        self.advance();
                        self.read_until('\n');
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::LineComment);
                    } else if nch == self.special_char {
                        self.advance();
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::Special);
                    } else if is_identifier_start(nch) {
                        // Possibly a named block or macro.
                        let after_pct = pct_start;
                        let id_start = self.pos;
                        let id_end = self.get_identifier_end(id_start);
                        self.pos = id_end;
                        let (maybe_after, a_line, a_col) = self.peek_char_and_pos();
                        if let Some(ma) = maybe_after {
                            if ma == '{' {
                                self.advance();
                                self.emit_token(
                                    after_pct,
                                    self.pos - after_pct,
                                    TokenKind::BlockOpen,
                                );
                                self.state_stack.push(State::Block);
                                return true;
                            } else if ma == '}' {
                                if self.state_stack.len() <= 1 {
                                    self.error_here(
                                        a_line,
                                        a_col.saturating_sub(1),
                                        "Unmatched block close: no open block",
                                    );
                                }
                                self.advance();
                                self.emit_token(
                                    after_pct,
                                    self.pos - after_pct,
                                    TokenKind::BlockClose,
                                );
                                return false;
                            } else if ma == '(' {
                                self.advance();
                                self.emit_token(after_pct, self.pos - after_pct, TokenKind::Macro);
                                self.state_stack.push(State::Macro);
                                return true;
                            } else {
                                self.emit_token(
                                    after_pct,
                                    self.pos - after_pct,
                                    TokenKind::Text,
                                );
                            }
                        } else {
                            self.emit_token(after_pct, self.pos - after_pct, TokenKind::Text);
                        }
                    } else {
                        self.error_here(
                            nxt_line,
                            nxt_col,
                            &format!("Unrecognized char after '{}' in block", self.special_char),
                        );
                        self.emit_token(pct_start, 1, TokenKind::Text);
                    }
                } else {
                    // There is nothing after the special char, so emit it as text.
                    self.emit_token(pct_start, 1, TokenKind::Text);
                    return false;
                }
                text_start = self.pos;
            } else {
                // Normal text.
                self.advance();
            }
        }

        // At EOF flush any leftover text.
        if self.pos > text_start {
            self.emit_token(text_start, self.pos - text_start, TokenKind::Text);
        }
        false
    }

    /// Handle a "%(" token sequence representing a variable.
    fn handle_var(&mut self, start: usize, line: usize, col: usize) {
        // Consume the '('.
        self.advance();
        let ident_start = self.pos;
        let ident_end = self.get_identifier_end(ident_start);
        if ident_end > ident_start {
            self.pos = ident_end;
            // If next char is ')', then we have a valid var.
            if let (Some(')'), c_line, c_col) = self.peek_char_and_pos() {
                self.record_position(c_line, c_col);
                self.advance();
                self.emit_token(start, self.pos - start, TokenKind::Var);
                return;
            } else {
                self.error_here(line, col, "Var missing closing ')'");
            }
        } else {
            self.error_here(
                line,
                col,
                &format!("Var missing identifier after '{}('", self.special_char),
            );
        }
        // Fallback: emit the consumed characters as text.
        self.emit_token(start, self.pos - start, TokenKind::Text);
    }

    //--------------------------------------------------------------------------
    // MACRO STATE
    //--------------------------------------------------------------------------
    fn run_macro_state(&mut self) -> bool {
        while let (Some(ch), line, col) = self.peek_char_and_pos() {
            self.record_position(line, col);

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
                while let (Some(wch), _, _) = self.peek_char_and_pos() {
                    if !is_whitespace(wch) {
                        break;
                    }
                    self.advance();
                }
                self.emit_token(ws_start, self.pos - ws_start, TokenKind::Space);
            } else if ch == self.special_char {
                let (pct_char, pct_line, pct_col) = self.advance().unwrap();
                self.record_position(pct_line, pct_col);

                let pct_start = self.pos - pct_char.len_utf8();
                let (nch_opt, nxt_line, nxt_col) = self.peek_char_and_pos();
                if let Some(nch) = nch_opt {
                    if nch == '(' {
                        self.handle_var(pct_start, pct_line, pct_col);
                    } else if nch == '{' {
                        self.advance();
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::BlockOpen);
                        self.state_stack.push(State::Block);
                        return true;
                    } else if nch == '}' {
                        if self.state_stack.len() <= 1 {
                            self.error_here(
                                nxt_line,
                                nxt_col.saturating_sub(1),
                                "Unmatched block close: no open block",
                            );
                        }
                        self.advance();
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::BlockClose);
                        return false;
                    } else if nch == '/' {
                        self.advance();
                        if let (Some(slch), sl_line, sl_col) = self.peek_char_and_pos() {
                            self.record_position(sl_line, sl_col);
                            if slch == '/' {
                                self.advance();
                                self.read_until('\n');
                                self.emit_token(
                                    pct_start,
                                    self.pos - pct_start,
                                    TokenKind::LineComment,
                                );
                            } else if slch == '*' {
                                self.advance();
                                self.emit_token(
                                    pct_start,
                                    self.pos - pct_start,
                                    TokenKind::CommentOpen,
                                );
                                self.state_stack.push(State::Comment);
                                return true;
                            } else {
                                self.error_here(
                                    sl_line,
                                    sl_col,
                                    &format!(
                                        "Unexpected char after '{}{}' in macro",
                                        self.special_char, "/"
                                    ),
                                );
                                self.emit_token(pct_start, self.pos - pct_start, TokenKind::Text);
                            }
                        }
                    } else if nch == '-' {
                        self.advance();
                        if let (Some(d2), dl, dc) = self.peek_char_and_pos() {
                            self.record_position(dl, dc);
                            if d2 == '-' {
                                self.advance();
                                self.read_until('\n');
                                self.emit_token(
                                    pct_start,
                                    self.pos - pct_start,
                                    TokenKind::LineComment,
                                );
                            } else {
                                self.error_here(
                                    dl,
                                    dc,
                                    &format!(
                                        "Unexpected char after '{}{}' in macro",
                                        self.special_char, "-"
                                    ),
                                );
                                self.emit_token(pct_start, self.pos - pct_start, TokenKind::Text);
                            }
                        }
                    } else if nch == '#' {
                        self.advance();
                        self.read_until('\n');
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::LineComment);
                    } else if nch == self.special_char {
                        // Handle a doubled special char, e.g. "%%"
                        self.advance();
                        self.emit_token(pct_start, self.pos - pct_start, TokenKind::Special);
                    } else if is_identifier_start(nch) {
                        let after_pct = pct_start;
                        let id_start = self.pos;
                        let id_end = self.get_identifier_end(id_start);
                        self.pos = id_end;
                        let (post_char, p_line, p_col) = self.peek_char_and_pos();
                        if let Some(pc) = post_char {
                            if pc == '{' {
                                self.advance();
                                self.emit_token(
                                    after_pct,
                                    self.pos - after_pct,
                                    TokenKind::BlockOpen,
                                );
                                self.state_stack.push(State::Block);
                                return true;
                            } else if pc == '}' {
                                if self.state_stack.len() <= 1 {
                                    self.error_here(
                                        p_line,
                                        p_col.saturating_sub(1),
                                        "Unmatched block close: no open block",
                                    );
                                }
                                self.advance();
                                self.emit_token(
                                    after_pct,
                                    self.pos - after_pct,
                                    TokenKind::BlockClose,
                                );
                                return false;
                            } else if pc == '(' {
                                self.advance();
                                self.emit_token(after_pct, self.pos - after_pct, TokenKind::Macro);
                                self.state_stack.push(State::Macro);
                                return true;
                            } else {
                                self.emit_token(
                                    after_pct,
                                    self.pos - after_pct,
                                    TokenKind::Text,
                                );
                            }
                        } else {
                            self.emit_token(after_pct, self.pos - after_pct, TokenKind::Text);
                        }
                    } else {
                        self.error_here(
                            nxt_line,
                            nxt_col,
                            &format!("Unrecognized char after '{}' in macro", self.special_char),
                        );
                        self.emit_token(pct_start, 1, TokenKind::Text);
                    }
                } else {
                    self.error_here(
                        pct_line,
                        pct_col,
                        &format!(
                            "EOF after '{}' in macro, incomplete token",
                            self.special_char
                        ),
                    );
                    self.emit_token(pct_start, 1, TokenKind::Text);
                    return false;
                }
            } else if is_identifier_start(ch) {
                let start_id = self.pos;
                let end_id = self.get_identifier_end(start_id);
                self.pos = end_id;
                self.emit_token(start_id, end_id - start_id, TokenKind::Ident);
            } else {
                // Consume text until a punctuation char or the special char is encountered.
                let start_o = self.pos;
                while let (Some(ch2), _, _) = self.peek_char_and_pos() {
                    if is_whitespace(ch2)
                        || matches!(ch2, ')' | ',' | '=')
                        || ch2 == self.special_char
                    {
                        break;
                    }
                    self.advance();
                }
                let length = self.pos - start_o;
                self.emit_token(start_o, length, TokenKind::Text);
            }

            if self.state_stack.last() != Some(&State::Macro) {
                return false;
            }
        }
        false
    }

    //--------------------------------------------------------------------------
    // COMMENT STATE (using the state stack for nested comments)
    //--------------------------------------------------------------------------
    fn run_comment_state(&mut self) -> bool {
        let comment_text_start = self.pos;
        while self.pos < self.bytes.len() {
            // Check for nested comment open: "{special_char}/*"
            let open_delim = format!("{}/*", self.special_char);
            if self.starts_with(&open_delim) {
                // Flush any text before the nested comment.
                if self.pos > comment_text_start {
                    self.emit_token(
                        comment_text_start,
                        self.pos - comment_text_start,
                        TokenKind::Text,
                    );
                }
                let delim_start = self.pos;
                self.consume_str(&open_delim);
                self.emit_token(delim_start, self.pos - delim_start, TokenKind::CommentOpen);
                self.state_stack.push(State::Comment);
                return true;
            }
            // Check for comment close: "{special_char}*/"
            let close_delim = format!("{}*/", self.special_char);
            if self.starts_with(&close_delim) {
                if self.pos > comment_text_start {
                    self.emit_token(
                        comment_text_start,
                        self.pos - comment_text_start,
                        TokenKind::Text,
                    );
                }
                let delim_start = self.pos;
                self.consume_str(&close_delim);
                self.emit_token(delim_start, self.pos - delim_start, TokenKind::CommentClose);
                return false;
            }
            self.advance();
        }
        // At EOF flush any leftover text before reporting error.
        if self.pos > comment_text_start {
            self.emit_token(
                comment_text_start,
                self.pos - comment_text_start,
                TokenKind::Text,
            );
        }
        self.error_here(self.line, self.column, "Unclosed comment");
        false
    }
}
