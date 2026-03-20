-- editors/neovim/asciidoc.lua
--
-- Registers the asciidoc + asciidoc_inline tree-sitter parsers.
-- Installed by editors/neovim/install.py to:
--   ~/.config/nvim/after/plugin/asciidoc.lua
--
-- Usage (first time or after grammar changes):
--   :TSInstall asciidoc asciidoc_inline

local ok, parsers = pcall(require, "nvim-treesitter.parsers")
if not ok then
  vim.notify("nvim-treesitter not found — skipping asciidoc parser registration", vim.log.levels.WARN)
  return
end

local repo = "https://github.com/cathaysia/tree-sitter-asciidoc"
local rev  = "14e660bacac69a905e71ab1041eb64eb266a6112"

parsers.get_parser_configs().asciidoc = {
  install_info = {
    url      = repo,
    files    = { "tree-sitter-asciidoc/src/parser.c",
                 "tree-sitter-asciidoc/src/scanner.c" },
    location = "tree-sitter-asciidoc",
    revision = rev,
    generate_requires_npm        = false,
    requires_generate_from_grammar = false,
  },
  filetype = "asciidoc",
}

parsers.get_parser_configs().asciidoc_inline = {
  install_info = {
    url      = repo,
    files    = { "tree-sitter-asciidoc_inline/src/parser.c",
                 "tree-sitter-asciidoc_inline/src/scanner.c" },
    location = "tree-sitter-asciidoc_inline",
    revision = rev,
    generate_requires_npm        = false,
    requires_generate_from_grammar = false,
  },
  filetype = "asciidoc",
}

vim.filetype.add({
  extension = {
    adoc     = "asciidoc",
    asciidoc = "asciidoc",
  },
})
