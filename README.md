# syms

Fast project-wide symbol search using tree-sitter. Like ripgrep, but for symbols.

```
$ syms
rs   fn         [pub] extract_symbols    src/parser.rs:12:34:8      pub fn extract_symbols(path: &Arc<Path>, source: &[u8], lang: Language) -> Result<Vec<Symbol>> {
rs   method     [pub] Language.from_path  src/language.rs:51:53:12   pub fn from_path(path: &Path) -> Option<Self> {
tsx  component  [pub] App                src/App.tsx:3:5:17          export function App({ name }: { name: string }) {
tsx  hook       [pub] useAuth            src/hooks.ts:7:9:17         export function useAuth() {
py   test       [pub] test_login_flow    tests/test_app.py:4:5:5    def test_login_flow():
go   method     [pub] Server.Start       main.go:7:9:18             func (s *Server) Start() error {
```

## Install

```
cargo install syms
```

## Features

- **14 languages** built-in: Rust, TypeScript, TSX, JavaScript, Go, Python, C, C++, Java, Ruby, PHP, Bash, CSS, Lua
- **Custom languages** via `~/.config/syms/languages/*.toml` (any tree-sitter grammar)
- **Symbol signatures** — full declaration, not just the name
- **Parent context** — `Class.method`, `Impl.function`, `Receiver.Method`
- **Visibility** — `[pub]`/`[prv]` tags, `--exported` filter
- **Semantic kinds** — React components, hooks, test functions
- **fzf/skim integration** — `--fzf`/`--sk` with tree-sitter-aware preview showing the exact symbol body
- **JSON output** — `-o json` or `-o jsonl` for tool integration
- **Zero config** — works instantly on any project

## Usage

```bash
syms                              # all symbols in current directory
syms src/                         # search specific directory
syms --kind fn,method             # only functions and methods
syms --lang rs,go --exported      # public Rust and Go symbols
syms --kind test                  # all test functions
syms --kind component             # React components (TSX)
syms --kind hook                  # React hooks
syms -o json | jq '.[]'           # JSON output
syms -o jsonl                     # streaming JSON (one per line)
syms --fzf                        # interactive search with fzf
syms --sk                         # interactive search with skim
```

## Filters

| Flag | Values |
|------|--------|
| `--kind` | `fn`, `method`, `class`, `struct`, `enum`, `type`, `interface`, `const`, `mod`, `trait`, `component`, `hook`, `test` |
| `--lang` | `rs`, `ts`, `tsx`, `js`, `go`, `py`, `c`, `cpp`, `java`, `rb`, `php`, `sh`, `css`, `lua` |
| `--exported` | only public/exported symbols |
| `-o` | `json`, `jsonl` |

## fzf/skim Preview

`syms --fzf` launches fzf with a tree-sitter-aware preview that shows the **exact symbol body** — not "10 lines around the match", but the complete function/struct/class from the AST.

## Custom Languages

Add any tree-sitter grammar via a TOML config:

```toml
# ~/.config/syms/languages/nix.toml
extensions = ["nix"]
parser = "/path/to/nix.so"

[[queries]]
kind = "constant"
query = '(binding (attrpath (identifier) @name)) @def'
```

Compatible with grammars from helix, neovim, or `tree-sitter build`.

### Injections

Custom languages can declare a tree-sitter injection query to surface symbols
inside nested language regions (e.g. kakscript blocks inside `provide-module`,
`define-command`, or `hook` bodies). Point at an injections file, or inline the
query:

```toml
# ~/.config/syms/languages/kak.toml
extensions = ["kak"]
parser = "/path/to/kak.so"
injections_path = "/path/to/tree-sitter-kak/queries/injections.scm"
```

Injection targets resolve by language name — the string in `#set!
injection.language "<name>"` (or a `@injection.language` capture) is matched
against built-in languages (`"rust"`, `"bash"`, `"python"`, …) and the `name` /
`short` of any loaded custom language. Recursion is capped at 8 levels.

## License

MIT
