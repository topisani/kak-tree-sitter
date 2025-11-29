# Changelog

This is the changelog of the **kak-tree-sitter** project, which is composed of three sub-projects:

- `kak-tree-sitter`, the binary bridging **tree-sitter** with **Kakoune**.
- `ktsctl`, the CLI companion.
- `kak-tree-sitter-config`, the library used to parse and use the configuration.

> For older versions of the changelogs, please refer to [CHANGELOG.old.md](./CHANGELOG.old.md)

## kak-tree-sitter v3.0.0

### Major changes

- Many internal and structural changes (new configuration structure).
- Switch to [tree-house](https://github.com/helix-editor/tree-house). The switch boosts performance massively
  and is a building stone for upcoming features (partial updates).

### Minor changes

- Add more logs and timer-based logging.

### Bugfixes

- Languages are now lazy-loaded, which means that opening Kakoune on a scratch buffer will not cause any
  grammars nor queries to load. Loading happens when a buffer is created with a recognized and accepted
  file type by KTS.
- Reimplementation of async IO decoupling, especially via triple-buffering, to prevent freezing Kakoune if
  a bug is present in a grammar, causing KTS to take an abnormal time processing the buffer, highlights,
  queries, etc.
- Text-objects are correctly sent.
- Optimizes memory usage and scheme to prevent allocating during highlighting, massively boosting
  performances.

### Commit set

- [cdcfb42](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/cdcfb42) Fix upper byte boundary in highlighting code.
- [18c6e80](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/18c6e80) Properly load grammars.
- [b5771db](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/b5771db) Do not return highlights if there’s none computed.
- [abfeefe](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/abfeefe) Make tree-house grammar error more verbose.
- [0cd2f51](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/0cd2f51) Fix symbol loading issue when the language name has a dot in it.
- [14fce0a](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/14fce0a) Fix typo in debug log.
- [8bee812](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/8bee812) Fix highlighting logic with tree-house.
- [7ee3797](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/7ee3797) Fix tests regarding highlights with tree-house.
- [a459d17](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/a459d17) Fix face definition.
- [61c58d1](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/61c58d1) Convert to ropey/tree-house.
- [cc55d3e](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/cc55d3e) Prepare switching to tree-house.
- [f946149](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/f946149) ktsctl: operate on many languages at once.
- [a5bda78](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/a5bda78) Abort early after updating a buffer if it didn’t change.
- [eea7d5c](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/eea7d5c) Add tests for the triple buffer.
- [ed00fe4](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/ed00fe4) Implement triple-buffering and remove back-buffers.
- [3fcc99c](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/3fcc99c) Make fifo copying more explicit.
- [6911185](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/6911185) Fix text-objects not correctly sent.
- [ad15fdb](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/ad15fdb) Refactor IOHandler into its own module.
- [5fb0060](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/5fb0060) Reimplement proper async IO decoupling.
- [43b3a3f](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/43b3a3f) Remove trace logs and added some timer-based debug logs.
- [5d9295e](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/5d9295e) Change configuration to make grammars dissociate from languages.
- [fcab935](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/fcab935) Make grammars shared via Rc.
- [ab3874f](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/ab3874f) Automatically generate face definition based on the config.
- [53e4dde](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/53e4dde) Fix typos.
- [be260eb](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/be260eb) [feature/22] Remove box type ascription when creating lazy languages.
- [73a4eeb](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/73a4eeb) [feature/22] Massively simplify on-demand loading with LazyCell.
- [846ab1a](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/846ab1a) [feature/22] (disgusting) version of on-demand loading of languages.
- [4a6cce7](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/4a6cce7) Fix ts_constant_builtin face missing.

## ktsctl v3.0.0

### Major changes

- Configuration has changed to make grammars dissociate from languages.

### Minor changes

- All commands acting on languages now support a list of languages instead of a single one. For instance,
  `ktsctl sync rust toml`.
- Enhance reporting and stop hiding intermediate logs.
- Add `default-config` to print to standard output the content of the default configuration.

### Bugfixes

- Fix grammars not correctly synchronized.

### Commit set

- [1f9f3c3](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/1f9f3c3) [patch/61761] Rename Default into DefaultConfig.
- [4c565a9](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/4c565a9) ktsctl: Print default config
- [673fd2f](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/673fd2f) ktsctl: Enhance reporting.
- [7692f4d](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/7692f4d) Fix type (grammer -> queries).
- [f946149](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/f946149) ktsctl: operate on many languages at once.
- [fbedc85](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/fbedc85) [feature/27] Simplify the default config by removing optional fields.
- [6cbd42c](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/6cbd42c) Fix grammars not being correctly sync + TSX.
- [5d9295e](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/5d9295e) Change configuration to make grammars dissociate from languages.

## kak-tree-sitter-config v4.0.0

### Major changes

- The whole configuration is now optional besides `source`, which allows users to contribute / have custom
  configuration that is much smaller.
- Kakoune face definitions are now generated automatically from the config, so we do not have to hardcode
  `set-face` declarations, which occasionally caused inconsistency.
- Change configuration to make grammars dissociate from languages, allowing reuse for languages requiring the
  same grammar.
- Small internal change to allow to access the default configuration (mostly used by `ktsctl default-config`).

### Minor changes

- **typst**` is now supported in the default config.
- Add support for `rust-format-args`, allowing to highlight inside macros such as `println!` or `format!`.
- Add support for **jsonc**.
- Add support for **mermaid**.
- Add support for **lean**.
- Add support for **qml**.

### Bugfixes

- **C++** grammar has been updated as well as its queries (`nullptr -> null`).
- Fix **Rust** incorect highlights.
- Update **Typescript** query source.
- Fix **JSX** and **TSX** highlights, as well as capture groups for **JSX**.
- Fix **Vue** queries pin.
- Update **Ini**.
- Update **Markdown** configuration.

### Commit set

- [4c565a9](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/4c565a9) ktsctl: Print default config
- [29ee234](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/29ee234) Update markdown and markdown.inline.
- [24d80ca](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/24d80ca) Add qml language support
- [307fc50](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/307fc50) add config for lean
- [218762f](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/218762f) Fix configuration overrides / mandatory fields.
- [fbedc85](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/fbedc85) [feature/27] Simplify the default config by removing optional fields.
- [07477e7](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/07477e7) [feature/27] Make all GrammarConfig fields optional but source.
- [38a241f](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/38a241f) Fix typst query configuration
- [5fbc5df](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/5fbc5df) Bumped ini grammar
- [c5c1951](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/c5c1951) Add mermaid grammar
- [6cbd42c](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/6cbd42c) Fix grammars not being correctly sync + TSX.
- [c8314df](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/c8314df) Pin jsonc’ queries.
- [55a8f80](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/55a8f80) Add support for jsonc.
- [5d9295e](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/5d9295e) Change configuration to make grammars dissociate from languages.
- [395f229](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/395f229) Add missing capture groups for JSX.
- [7f598af](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/7f598af) Fix Vue queries pin.
- [84c46bf](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/84c46bf) Pin JSX and TSX to KTS ref.
- [2e5b784](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/2e5b784) Fix JSX / TSX highlights.scm.
- [ab3874f](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/ab3874f) Automatically generate face definition based on the config.
- [b50e52b](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/b50e52b) changed typescript query source
- [6b984af](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/6b984af) Add support for rust-format-args.
- [0534e34](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/0534e34) Bump Rust queries (fix incorrect highlights).
- [4a772dc](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/4a772dc) Update cpp grammar and point queries to commit featuring nullptr->null update.
- [22d0353](https://git.sr.ht/~hadronized/kak-tree-sitter/commit/22d0353) Add typst to default config
