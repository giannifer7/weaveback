// crates/weaveback-macro/src/parser/mod.rs — generated from parser.adoc
use crate::line_index::LineIndex;
use crate::types::{ASTNode, NodeKind, ParseNode, Token, TokenKind};
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use thiserror::Error;

#[cfg(test)]
mod tests;

/// The parser-specific error type.
#[derive(Error, Debug)]
pub enum ParserError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid token data: {0}")]
    TokenData(String),

    #[error("Parse error: {0}")]
    Parse(String),
}

impl From<String> for ParserError {
    fn from(s: String) -> Self {
        ParserError::TokenData(s)
    }
}
#[cfg(test)]
impl Token {
    pub fn to_json(&self) -> String {
        format!(
            "[{},{},{},{}]",
            self.src, self.kind as i32, self.pos, self.length
        )
    }
}

#[cfg(test)]
impl ParseNode {
    pub fn to_json(&self) -> String {
        format!(
            "[{},{},{},{},{}]",
            self.kind as i32,
            self.src,
            self.token.to_json(),
            self.end_pos,
            if self.parts.is_empty() {
                "[]".to_string()
            } else {
                format!(
                    "[{}]",
                    self.parts
                        .iter()
                        .map(|i| i.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                )
            }
        )
    }
}

/// Stack frame state.  Each variant owns its termination tokens;
/// everything else falls through to the shared opener/leaf handler.
///
/// `tag_len == 0` means an anonymous block (`%{`/`%}`).
///
/// `Macro` and `Param` are always paired: `Macro` sits below `Param` on the
/// stack.  `Param` receives individual tokens; `Macro` is only ever visible
/// after `Param` is popped (at `)`) so that `handle_param` can verify the
/// expected stack shape.
#[derive(Debug, Clone, Copy)]
enum ParserState {
    Block { tag_pos: usize, tag_len: usize },
    Macro,
    Param,
    Comment,
}

/// Bundles the source bytes with a borrowed `LineIndex`.
/// The index is built once by the caller (who may already need it for lexer
/// error formatting) and passed in, so the O(n) newline scan never happens
/// more than once per source string.
struct ParseContext<'a> {
    content: &'a [u8],
    line_index: &'a LineIndex,
}

impl<'a> ParseContext<'a> {
    fn new(content: &'a [u8], line_index: &'a LineIndex) -> Self {
        Self { content, line_index }
    }

    fn line_col(&self, pos: usize) -> (usize, usize) {
        self.line_index.line_col(pos)
    }

    /// Compare two tag spans. Anonymous (len == 0) matches anonymous only.
    /// Returns `false` — not a panic — if either span is out of bounds.
    fn tags_match(&self, (ap, al): (usize, usize), (bp, bl): (usize, usize)) -> bool {
        if al != bl {
            return false;
        }
        if al == 0 {
            return true;
        }
        if ap + al > self.content.len() || bp + bl > self.content.len() {
            return false;
        }
        self.content[ap..ap + al] == self.content[bp..bp + bl]
    }

    /// Return the tag as a `&str` for error messages.
    fn tag_str(&self, pos: usize, len: usize) -> &str {
        if len == 0 {
            return "";
        }
        debug_assert!(
            pos + len <= self.content.len(),
            "tag span OOB: {pos}+{len} > {}",
            self.content.len()
        );
        std::str::from_utf8(self.content.get(pos..pos + len).unwrap_or(&[]))
            .unwrap_or("")
    }
}

/// Format a block tag for error messages.
/// Anonymous blocks (`tag == ""`) render as `(anonymous)`;
/// named blocks render as `%name{` (open) or `%name}` (close).
fn block_tag_label(tag: &str, brace: char) -> String {
    if tag.is_empty() {
        "(anonymous)".to_string()
    } else {
        format!("%{tag}{brace}")
    }
}

pub struct Parser {
    nodes: Vec<ParseNode>,
    stack: Vec<(ParserState, usize)>,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    pub fn new() -> Self {
        Parser {
            nodes: Vec::new(),
            stack: Vec::new(),
        }
    }

    fn create_node(&mut self, kind: NodeKind, src: u32, token: Token) -> usize {
        let node = ParseNode {
            kind,
            src,
            token,
            end_pos: token.end(),
            parts: Vec::new(),
        };
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    fn add_child(&mut self, parent_idx: usize, child_idx: usize) {
        if let Some(parent) = self.nodes.get_mut(parent_idx) {
            parent.parts.push(child_idx);
        }
    }

    fn create_add_node(&mut self, kind: NodeKind, src: u32, token: Token) -> usize {
        let new_idx = self.create_node(kind, src, token);
        // Attach it to the node on top of the stack
        if let Some(&(_, parent_idx)) = self.stack.last() {
            self.add_child(parent_idx, new_idx);
        }
        new_idx
    }

    /// Convenience: `create_add_node` + push a new stack frame in one call.
    fn push_node(&mut self, state: ParserState, kind: NodeKind, src: u32, token: Token) -> usize {
        let idx = self.create_add_node(kind, src, token);
        self.stack.push((state, idx));
        idx
    }

    /// Set `end_pos` on the node currently at the top of the stack.
    /// Must be called *before* the corresponding `stack.pop()`.
    /// Returns `Err` rather than panicking so callers can propagate cleanly.
    fn close_top(&mut self, end: usize) -> Result<(), ParserError> {
        let (_, node_idx) = *self.stack.last().ok_or_else(|| {
            ParserError::Parse("internal error: close_top on empty stack".into())
        })?;
        self.nodes
            .get_mut(node_idx)
            .ok_or_else(|| {
                ParserError::Parse(format!(
                    "internal error: close_top node {node_idx} not in arena"
                ))
            })?
            .end_pos = end;
        Ok(())
    }

    /// Close all open nodes and clear the stack.  Called on both error and
    /// normal termination paths to keep the tree structurally consistent.
    fn unwind_stack(&mut self, end: usize) {
        while let Some(&(_, node_idx)) = self.stack.last() {
            debug_assert!(node_idx < self.nodes.len(), "unwind_stack: node_idx {node_idx} OOB");
            self.nodes[node_idx].end_pos = end;
            self.stack.pop();
        }
    }
    /// Extract the tag sub-span from a `BlockOpen` or `BlockClose` token.
    /// For `%{` / `%}` (length 2) the tag is empty (tag_len == 0).
    /// For `%foo{` / `%foo}` (length > 2) the tag is bytes
    /// [pos+special_len .. pos+length-1].
    fn block_tag(token: &Token, content: &[u8]) -> (usize, usize) {
        let special_len = content
            .get(token.pos..)
            .and_then(|tail| std::str::from_utf8(tail).ok())
            .and_then(|s| s.chars().next())
            .map(|c| c.len_utf8())
            .unwrap_or(1);
        let tag_len = token.length.saturating_sub(special_len + 1);
        (token.pos + special_len, tag_len)
    }
    /// Handle a token when the top of the stack is a `Block`.
    /// `tag_pos`/`tag_len` come directly from the caller's pattern match —
    /// no second stack lookup needed.
    /// Returns `Ok(true)` if the token was consumed (caller should `continue`).
    fn handle_block(
        &mut self,
        token: Token,
        ctx: &ParseContext,
        tag_pos: usize,
        tag_len: usize,
    ) -> Result<bool, ParserError> {
        if token.kind != TokenKind::BlockClose {
            return Ok(false);
        }
        let close_tag = Self::block_tag(&token, ctx.content);
        let open_tag = (tag_pos, tag_len);
        if !ctx.tags_match(open_tag, close_tag) {
            let (ol, oc) = ctx.line_col(tag_pos);
            let (cl, cc) = ctx.line_col(token.pos);
            let open_label = block_tag_label(ctx.tag_str(open_tag.0, open_tag.1), '{');
            let close_label = block_tag_label(ctx.tag_str(close_tag.0, close_tag.1), '}');
            let err = ParserError::Parse(format!(
                "{cl}:{cc}: block tag mismatch: '{close_label}' does not close '{open_label}' (opened at {ol}:{oc})",
            ));
            self.unwind_stack(token.end());
            return Err(err);
        }
        self.close_top(token.end())?;
        self.stack.pop();
        Ok(true)
    }
    /// Handle a token when the top of the stack is a `Param`.
    /// Returns `Ok(true)` if the token was consumed (caller should `continue`).
    fn handle_param(&mut self, token: Token) -> Result<bool, ParserError> {
        match token.kind {
            TokenKind::Comma => {
                // Close current Param, open next Param.
                self.close_top(token.pos)?;
                self.stack.pop();
                self.push_node(ParserState::Param, NodeKind::Param, token.src, token);
                Ok(true)
            }
            TokenKind::CloseParen => {
                // Close Param, then Macro (which must be directly below).
                self.close_top(token.end())?;
                self.stack.pop();
                match self.stack.last() {
                    Some((ParserState::Macro, _)) => {
                        self.close_top(token.end())?;
                        self.stack.pop();
                    }
                    _ => {
                        self.unwind_stack(token.end());
                        return Err(ParserError::Parse(
                            "internal error: expected Macro below Param".into(),
                        ));
                    }
                }
                Ok(true)
            }
            TokenKind::Ident => {
                self.create_add_node(NodeKind::Ident, token.src, token);
                Ok(true)
            }
            TokenKind::Space => {
                self.create_add_node(NodeKind::Space, token.src, token);
                Ok(true)
            }
            TokenKind::Equal => {
                self.create_add_node(NodeKind::Equal, token.src, token);
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    /// Handle a token when the top of the stack is a `Comment`.
    /// Comment state always consumes every token, so no bool return is needed.
    fn handle_comment(&mut self, token: Token) -> Result<(), ParserError> {
        match token.kind {
            TokenKind::CommentClose => {
                self.close_top(token.end())?;
                self.stack.pop();
            }
            TokenKind::CommentOpen => {
                // Nested comment.
                self.push_node(ParserState::Comment, NodeKind::BlockComment, token.src, token);
            }
            _ => {} // Inside a comment everything is opaque.
        }
        Ok(())
    }
    /// Main parse function.
    /// `content` is the raw source bytes — used for block-tag comparison and diagnostics.
    /// `line_index` is borrowed from the caller, who may have built it already for
    /// lexer-error formatting, so the O(n) newline scan happens at most once per source.
    pub fn parse(&mut self, tokens: &[Token], content: &[u8], line_index: &LineIndex) -> Result<(), ParserError> {
        self.nodes.clear();
        self.stack.clear();

        if tokens.is_empty() {
            return Ok(());
        }

        let ctx = ParseContext::new(content, line_index);

        // Root is a synthetic Block: it has no source token.
        let root_idx = self.create_node(
            NodeKind::Block,
            tokens[0].src,
            Token::synthetic(tokens[0].src, 0),
        );
        self.stack.push((ParserState::Block { tag_pos: 0, tag_len: 0 }, root_idx));

        for token in tokens {
            let token = *token;

            // EOF is a structural sentinel, not an AST node.
            if token.kind == TokenKind::EOF {
                break;
            }

            let consumed = match self.stack.last().map(|&(st, _)| st) {
                Some(ParserState::Block { tag_pos, tag_len }) => {
                    self.handle_block(token, &ctx, tag_pos, tag_len)?
                }
                Some(ParserState::Param) => self.handle_param(token)?,
                Some(ParserState::Macro) => {
                    // Macro is always below Param; receiving a token here is an
                    // internal invariant violation.
                    self.unwind_stack(token.end());
                    return Err(ParserError::Parse(
                        "internal error: token received in Macro state (Param expected on top)"
                            .into(),
                    ));
                }
                Some(ParserState::Comment) => {
                    self.handle_comment(token)?;
                    true
                }
                None => {
                    return Err(ParserError::Parse(
                        "internal error: empty parser stack".into(),
                    ));
                }
            };

            if consumed {
                continue;
            }

            // Tokens that open new structure or become leaf nodes.
            match token.kind {
                TokenKind::BlockOpen => {
                    let (tag_pos, tag_len) = Self::block_tag(&token, ctx.content);
                    self.push_node(
                        ParserState::Block { tag_pos, tag_len },
                        NodeKind::Block,
                        token.src,
                        token,
                    );
                }
                TokenKind::Macro => {
                    // Push Macro node, then an initial synthetic Param node on top.
                    self.push_node(ParserState::Macro, NodeKind::Macro, token.src, token);
                    self.push_node(
                        ParserState::Param,
                        NodeKind::Param,
                        token.src,
                        Token::synthetic(token.src, token.pos),
                    );
                }
                TokenKind::CommentOpen => {
                    self.push_node(ParserState::Comment, NodeKind::BlockComment, token.src, token);
                }
                TokenKind::Var => {
                    self.create_add_node(NodeKind::Var, token.src, token);
                }
                TokenKind::LineComment => {
                    self.create_add_node(NodeKind::LineComment, token.src, token);
                }
                _ => {
                    // Structural tokens must be handled above; anything else
                    // (Text, Space, Special, stray punctuation) falls through as Text.
                    debug_assert!(
                        !matches!(
                            token.kind,
                            TokenKind::BlockOpen
                                | TokenKind::CommentOpen
                                | TokenKind::Macro
                                | TokenKind::Var
                                | TokenKind::LineComment
                                | TokenKind::EOF
                        ),
                        "unexpected structural token in Text fallback: {:?}",
                        token.kind
                    );
                    self.create_add_node(NodeKind::Text, token.src, token);
                }
            }
        }

        let end = tokens.last().map(|t| t.end()).unwrap_or(0);

        // Report unclosed non-root structures (stack[0] is always the root block).
        if self.stack.len() > 1 {
            let err = match self.stack.last().map(|&(st, idx)| (st, idx)) {
                Some((ParserState::Block { tag_pos, tag_len }, _)) => {
                    let label = block_tag_label(ctx.tag_str(tag_pos, tag_len), '{');
                    let (line, col) = ctx.line_col(tag_pos);
                    ParserError::Parse(format!("{line}:{col}: unclosed block '{label}'"))
                }
                Some((ParserState::Macro, idx)) | Some((ParserState::Param, idx)) => {
                    let pos = self.nodes.get(idx)
                        .expect("node index from stack must exist in arena")
                        .token.pos;
                    let (line, col) = ctx.line_col(pos);
                    ParserError::Parse(format!("{line}:{col}: unclosed macro argument list"))
                }
                Some((ParserState::Comment, idx)) => {
                    let pos = self.nodes.get(idx)
                        .expect("node index from stack must exist in arena")
                        .token.pos;
                    let (line, col) = ctx.line_col(pos);
                    ParserError::Parse(format!("{line}:{col}: unclosed block comment '%/*'"))
                }
                None => unreachable!(),
            };
            self.unwind_stack(end);
            return Err(err);
        }

        // Normal termination: close the root block.
        self.close_top(end)?;
        self.stack.pop();

        Ok(())
    }
    fn parse_token_from_parts(parts: Vec<&str>) -> Result<Token, ParserError> {
        if parts.len() != 4 {
            return Err(ParserError::TokenData(format!(
                "Invalid token data: {}",
                parts.join(",")
            )));
        }
        Ok(Token {
            src: parts[0]
                .parse()
                .map_err(|e| ParserError::TokenData(format!("Invalid src: {}", e)))?,
            kind: parts[1]
                .parse::<i32>()
                .map_err(|e| ParserError::TokenData(format!("Invalid kind: {}", e)))?
                .try_into()?,
            pos: parts[2]
                .parse()
                .map_err(|e| ParserError::TokenData(format!("Invalid pos: {}", e)))?,
            length: parts[3]
                .parse()
                .map_err(|e| ParserError::TokenData(format!("Invalid length: {}", e)))?,
        })
    }

    fn parse_tokens<I>(lines: I) -> Result<Vec<Token>, ParserError>
    where
        I: Iterator<Item = Result<String, std::io::Error>>,
    {
        let mut tokens = Vec::new();
        for line in lines {
            let line =
                line.map_err(|e| ParserError::TokenData(format!("Failed to read line: {}", e)))?;
            let parts: Vec<&str> = line.split(',').collect();
            tokens.push(Self::parse_token_from_parts(parts)?);
        }
        Ok(tokens)
    }

    pub fn read_tokens(path: &str) -> Result<Vec<Token>, ParserError> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        Self::parse_tokens(reader.lines())
    }

    pub fn read_tokens_from_stdin() -> Result<Vec<Token>, ParserError> {
        let stdin = io::stdin();
        Self::parse_tokens(stdin.lock().lines())
    }
    /// Get a reference to a node by index
    pub fn get_node(&self, idx: usize) -> Option<&ParseNode> {
        self.nodes.get(idx)
    }

    /// Get a mutable reference to a node by index
    pub fn get_node_mut(&mut self, idx: usize) -> Option<&mut ParseNode> {
        self.nodes.get_mut(idx)
    }

    pub fn get_node_info(&self, idx: usize) -> Option<(&ParseNode, NodeKind)> {
        self.nodes.get(idx).map(|node| (node, node.kind))
    }

    #[cfg(test)]
    pub fn to_json(&self) -> String {
        self.nodes
            .iter()
            .map(|node| node.to_json())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Get the root node's index (usually 0 if parse succeeded)
    pub fn get_root_index(&self) -> Option<usize> {
        if self.nodes.is_empty() {
            None
        } else {
            Some(0) // The root is always the first node
        }
    }

    /// Process AST including space stripping
    pub fn process_ast(&mut self, content: &[u8]) -> Result<ASTNode, String> {
        let root_idx = self
            .get_root_index()
            .ok_or_else(|| "Empty parse tree".to_string())?;

        crate::ast::strip_space_before_comments(content, self, root_idx)
            .map_err(|e| e.to_string())?;

        crate::ast::build_ast(self).map_err(|e| e.to_string())
    }

    /// Direct build without space stripping
    pub fn build_ast(&self) -> Result<ASTNode, String> {
        crate::ast::build_ast(self).map_err(|e| e.to_string())
    }

    /// Strip ending spaces from a node's token
    pub fn strip_ending_space(&mut self, content: &[u8], node_idx: usize) -> Result<(), String> {
        let node = self
            .get_node_mut(node_idx)
            .ok_or_else(|| format!("Node index {} not found", node_idx))?;

        let start = node.token.pos;
        let end = node.token.pos + node.token.length;
        if start >= content.len() {
            return Ok(());
        }

        let mut space_count = 0;
        for c in content[start..end.min(content.len())].iter().rev() {
            if *c == b'\n' || *c == b'\r' || *c == b' ' || *c == b'\t' {
                space_count += 1;
            } else {
                break;
            }
        }

        if space_count > 0 {
            node.token.length = node.token.length.saturating_sub(space_count);
            node.end_pos = node.token.pos + node.token.length;
        }

        Ok(())
    }

    #[cfg(test)]
    pub fn add_node(&mut self, node: ParseNode) -> usize {
        self.nodes.push(node);
        self.nodes.len() - 1
    }
}
