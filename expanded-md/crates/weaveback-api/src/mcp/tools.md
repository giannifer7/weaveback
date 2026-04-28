# MCP Tool Schemas

## Tool Schemas

The MCP `tools/list` response is large enough to deserve a separate owner.
Keeping it outside the runtime loop makes the dispatch logic easier to scan.

```rust
// <[mcp-tools]>=
pub(super) fn tools_list_result() -> Value {
    json!({
                        "tools": [
                            {
                                "name": "weaveback_trace",
                                "description": "Trace an output file line back to its original literate source. Returns src_file/src_line/src_col/kind. MacroArg spans include macro_name/param_name. MacroBody spans include macro_name and a def_locations array (all %def call sites). VarBinding spans include var_name and a set_locations array (all %set call sites). Use --col for sub-line token precision.",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "out_file": { "type": "string", "description": "Path to the generated file" },
                                        "out_line": { "type": "integer", "description": "1-indexed line number in the generated file" },
                                        "out_col":  { "type": "integer", "description": "1-indexed character position within the output line (default 1). Use to pinpoint a specific token." }
                                    },
                                    "required": ["out_file", "out_line"]
                                }
                            },
                            {
                                "name": "weaveback_apply_back",
                                "description": "Bulk baseline-reconciliation tool: propagate edits already made directly in gen/ files back to the literate source. Use this only when gen/ files have been edited by hand and you need to reconcile the baseline. For intentional fixes where you know what the source should look like, prefer weaveback_apply_fix (oracle-verified, surgical, no full rebuild needed). weaveback_apply_back diffs each modified gen/ file against its stored baseline, traces each changed line to its noweb+macro origin, and patches the literate source. Returns a report of what was patched, skipped, or needs manual attention.",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "files":   { "type": "array", "items": { "type": "string" }, "description": "Relative paths within gen/ to process (default: all modified files)" },
                                        "dry_run": { "type": "boolean", "description": "Show what would change without writing (default: false)" }
                                    },
                                    "required": []
                                }
                            },
                            {
                                "name": "weaveback_apply_fix",
                                "description": "**Preferred tool for all literate-source edits.** Apply a source edit (single line or multi-line range) and oracle-verify it produces the expected output before writing. Workflow: (1) use weaveback_trace to find src_file/src_line, (2) read the source, (3) call this tool with the replacement and the expected output line. The macro expander re-runs as an oracle — the file is written only if the expected output is produced, making the edit safe to apply without a full rebuild. Use apply_back only when you have already edited gen/ files directly and need to reconcile the baseline.",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "src_file":        { "type": "string",  "description": "Absolute path of the literate source file to edit" },
                                        "src_line":        { "type": "integer", "description": "1-indexed first line to replace in src_file" },
                                        "src_line_end":    { "type": "integer", "description": "1-indexed last line of the replacement range (inclusive, defaults to src_line for single-line edits)" },
                                        "new_src_line":    { "type": "string",  "description": "Replacement text when replacing a single line (without trailing newline)" },
                                        "new_src_lines":   { "type": "array", "items": { "type": "string" }, "description": "Replacement lines for multi-line edits (each element is one line without trailing newline); overrides new_src_line when present" },
                                        "out_file":        { "type": "string",  "description": "Generated file path (used for oracle lookup)" },
                                        "out_line":        { "type": "integer", "description": "1-indexed line in the generated file (oracle check point)" },
                                        "expected_output": { "type": "string",  "description": "The exact content of out_line expected after the fix (indent-stripped); oracle rejects the edit if this does not match" }
                                    },
                                    "required": ["src_file", "src_line", "out_file", "out_line", "expected_output"]
                                }
                            },
                            {
                                "name": "weaveback_chunk_context",
                                "description": "Return full context for a named noweb chunk: its body, the AsciiDoc section title breadcrumb, the full prose of the enclosing section (paragraphs, admonitions, design notes), bodies of all direct dependencies, reverse-dep names, output files, and recent git log entries. Use this before editing or reasoning about a chunk.",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "file": { "type": "string", "description": "Source file path (relative to project root), e.g. 'crates/weaveback/src/serve.adoc'" },
                                        "name": { "type": "string", "description": "Chunk name as it appears in the <<name>>= marker" },
                                        "nth":  { "type": "integer", "description": "0-based index for chunks defined multiple times (default 0)" }
                                    },
                                    "required": ["file", "name"]
                                }
                            },
                            {
                                "name": "weaveback_list_chunks",
                                "description": "List all chunk definitions in the project, optionally filtered to a single source file. Returns an array of { file, name, nth, def_start, def_end } objects.",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "file": { "type": "string", "description": "Source file to filter to (optional; omit for all files)" }
                                    },
                                    "required": []
                                }
                            },
                            {
                                "name": "weaveback_find_chunk",
                                "description": "Find which source file(s) define a given chunk name. Returns an array of { file, nth, def_start, def_end } objects.",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "name": { "type": "string", "description": "Chunk name to look up" }
                                    },
                                    "required": ["name"]
                                }
                            },
                            {
                                "name": "weaveback_lsp_definition",
                                "description": "Find the definition of a symbol at a given position in a generated file, and map it back to its original literate source. Requires rust-analyzer.",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "out_file": { "type": "string", "description": "Path to the generated file" },
                                        "line":     { "type": "integer", "description": "1-indexed line number" },
                                        "col":      { "type": "integer", "description": "1-indexed character position" }
                                    },
                                    "required": ["out_file", "line", "col"]
                                }
                            },
                            {
                                "name": "weaveback_lsp_references",
                                "description": "Find all references to a symbol at a given position in a generated file, and map them back to their original literate sources. Requires rust-analyzer.",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "out_file": { "type": "string", "description": "Path to the generated file" },
                                        "line":     { "type": "integer", "description": "1-indexed line number" },
                                        "col":      { "type": "integer", "description": "1-indexed character position" }
                                    },
                                    "required": ["out_file", "line", "col"]
                                }
                            },
                            {
                                "name": "weaveback_lsp_hover",
                                "description": "Get type information and documentation for a symbol at a given position in a generated file, mapped back to literate source. Requires rust-analyzer.",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "out_file": { "type": "string", "description": "Path to the generated file" },
                                        "line":     { "type": "integer", "description": "1-indexed line number" },
                                        "col":      { "type": "integer", "description": "1-indexed character position" }
                                    },
                                    "required": ["out_file", "line", "col"]
                                }
                            },
                            {
                                "name": "weaveback_lsp_diagnostics",
                                "description": "Get current compiler errors/warnings for a generated file, mapped back to original literate source lines.",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "out_file": { "type": "string", "description": "Path to the generated file" }
                                    },
                                    "required": ["out_file"]
                                }
                            },
                            {
                                "name": "weaveback_lsp_symbols",
                                "description": "List all semantic symbols (functions, structs, etc.) in a generated file, with their original literate source locations.",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "out_file": { "type": "string", "description": "Path to the generated file" }
                                    },
                                    "required": ["out_file"]
                                }
                            },
                                        {
                                            "name": "weaveback_search",
                                            "description": "Hybrid search over the prose in all literate source files. FTS5 and tags are always used; if prose embeddings were generated during tangle, semantic reranking is also applied. Returns ranked excerpts with file path, line range, tags, score, and contributing channels. Use this to discover which chunks or sections are relevant to a concept before calling weaveback_chunk_context. Supports FTS5 query syntax: AND, OR, NOT, phrase \"...\", prefix foo*.",
                                            "inputSchema": {
                                                "type": "object",
                                                "properties": {
                                                    "query": { "type": "string", "description": "Search terms (FTS5 syntax)" },
                                                    "limit": { "type": "integer", "description": "Maximum results to return (default 10)" }
                                                },
                                                "required": ["query"]
                                            }
                                        },
                                        {
                                            "name": "weaveback_list_tags",
                                            "description": "List all LLM-generated tags for prose blocks in the project. Returns each block's source file, line, block type, and comma-separated tags. Optionally filter to a single source file. Use this to explore the semantic landscape of the project or to find all blocks tagged with a given concept.",
                                            "inputSchema": {
                                                "type": "object",
                                                "properties": {
                                                    "file": { "type": "string", "description": "Optional: filter to this source file (plain relative path, e.g. crates/weaveback-tangle/src/db.adoc)" }
                                                }
                                            }
                                        },
                                        {
                                            "name": "weaveback_coverage",
                                            "description": "Get test coverage summary grouped by literate source chunks and sections, sorted by missed lines. Use this to prioritize what to test. Requires a valid lcov.info file. Note: if no lcov_path is provided, defaults to 'lcov.info'.",
                                            "inputSchema": {
                                                "type": "object",
                                                "properties": {
                                                    "lcov_path": { "type": "string", "description": "Path to the lcov.info file (defaults to lcov.info in the root directory)" }
                                                }
                                            }
                                        }
                                    ]
                                })
}
// @
```

