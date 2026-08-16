vim.g.mapleader = " "
vim.opt.number = true
vim.opt.relativenumber = true
vim.opt.termguicolors = true
vim.opt.signcolumn = "yes"
vim.opt.cursorline = true
vim.opt.scrolloff = 6
vim.opt.sidescrolloff = 8
vim.opt.expandtab = true
vim.opt.shiftwidth = 2
vim.opt.tabstop = 2
vim.opt.smartindent = true
vim.opt.ignorecase = true
vim.opt.smartcase = true
vim.opt.splitright = true
vim.opt.splitbelow = true
vim.opt.undofile = true
vim.opt.updatetime = 250

vim.cmd("colorscheme habamax")
vim.api.nvim_set_hl(0, "Normal", { bg = "#171923", fg = "#d9dcec" })
vim.api.nvim_set_hl(0, "NormalFloat", { bg = "#1b1e2a", fg = "#d9dcec" })
vim.api.nvim_set_hl(0, "FloatBorder", { bg = "#1b1e2a", fg = "#8f7aa8" })
vim.api.nvim_set_hl(0, "CursorLine", { bg = "#202330" })
vim.api.nvim_set_hl(0, "Visual", { bg = "#51576d" })
vim.api.nvim_set_hl(0, "LineNr", { fg = "#626880" })
vim.api.nvim_set_hl(0, "CursorLineNr", { fg = "#cba6f7", bold = true })
pcall(dofile, vim.fn.expand("~/.config/nvim/arcos-theme.lua"))

vim.keymap.set("n", "<leader>w", "<cmd>write<cr>", { desc = "Save file" })
vim.keymap.set("n", "<leader>q", "<cmd>quit<cr>", { desc = "Quit" })
vim.keymap.set("n", "<leader>e", vim.cmd.Ex, { desc = "File explorer" })
vim.keymap.set("n", "<C-h>", "<C-w>h")
vim.keymap.set("n", "<C-j>", "<C-w>j")
vim.keymap.set("n", "<C-k>", "<C-w>k")
vim.keymap.set("n", "<C-l>", "<C-w>l")
