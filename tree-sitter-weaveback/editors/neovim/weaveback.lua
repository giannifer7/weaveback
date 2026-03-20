-- editors/neovim/weaveback.lua
--
-- Registers the weaveback tree-sitter parser and filetype.
-- Installed by editors/neovim/install.py to:
--   ~/.config/nvim/after/plugin/weaveback.lua
--
-- Prerequisites: nvim-treesitter installed.
-- The grammar is built from the local checkout (no internet needed after clone).
--
-- Usage:
--   :TSInstall weaveback      (first time, or after grammar changes)
--   :TSUpdate weaveback       (re-build after grammar.js changes)

local ok, parsers = pcall(require, "nvim-treesitter.parsers")
if not ok then
  vim.notify("nvim-treesitter not found — skipping weaveback parser registration", vim.log.levels.WARN)
  return
end

-- Path to tree-sitter-weaveback/ inside the weaveback checkout.
-- install.py substitutes __GRAMMAR_DIR__ with the actual absolute path.
local grammar_dir = "__GRAMMAR_DIR__"

parsers.get_parser_configs().weaveback = {
  install_info = {
    url                          = grammar_dir,
    files                        = { "src/parser.c" },
    generate_requires_npm        = false,
    requires_generate_from_grammar = false,
  },
  filetype = "weaveback",
}

vim.filetype.add({
  extension = { weaveback = "weaveback" },
})
