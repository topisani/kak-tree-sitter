# Configuration

Both `kak-tree-sitter` and `ktsctl` ship with a default configuration. It is
possible to override the default options via the user configuration.

The `$XDG_CONFIG_HOME/kak-tree-sitter/config.toml` contains the user
configuration of both `kak-tree-sitter` and `ktsctl`. If you want to tweak
something, you can have a look at the
[default configuration file](https://git.sr.ht/~hadronized/kak-tree-sitter/tree/master/item/kak-tree-sitter-config/default-config.toml)
to know which path and values to pick from.

> The user and default configurations get merged, so you do not have to copy the
> default configuration to tweak it.

# Option paths

> For all the options that accept arguments like `{lang}`, those values are replaced
> at runtime by the actual value of the language the configuration option is for.

## `features`

This section contains enabled/disabled features. You can enable or disable a
given feature if you are not interested in it.

List of features:

| Feature        | Description                                                                                                              | Default |
| -------        | -----------                                                                                                              | ------- |
| `highlighting` | Enable highlighting. If set to `false`, can be overridden on the CLI with `--with-highlighting`.                         |  `true` |
| `text_objects` | Enable text-objects user modes and mappings. If set to `false`, can be overridden on the CLI with `--with-text-objects`. |  `true` |

## `highlight.groups`

The `highlight` section contains a single list, `groups`, which is used to list
every capture groups used by language queries. If you install a language with
queries containing new capture groups not already listed there, you need to add
them at the end of the list.

> Please consider contributing if you find a hole / missing capture group.

## `grammar`

The `grammar` table contains grammar-keyed configuration — e.g.
`grammar.rust`. For a given language `language.<lang>`, it must have an
associated `grammar.<lang>`, or a grammar mapped in `language.<lang>.grammar`
in the case of sharing grammars between different languages.

### `grammar.<lang>`

This section contains various information about how to fetch, compile and link a
grammar.

The following field(s) are mandatory and must always be provided:

| Field    | Description                                                                     |
| -----    | -----------                                                                     |
| `source` | the source from where to pick the grammar; see the [Sources](#sources) section. |

The following fields are optional and have a default value associated with.
Override only the values that need to:

| Field           | Description                                                                                                           | Default value                                                      |
| -----           | -----------                                                                                                           | -------------                                                      |
| `path`          | Path where to find the various source files. Should always be `src` but can require adjustments for monorepositories. | `src`                                                              |
| `compile`       | Compile command to use. Should always be `cc`.                                                                        | `cc`                                                               |
| `compile_args`  | Arguments to pass to `compile` to compile the grammar.                                                                | `["-c", "-fpic", "../parser.c", "-I", ".."]`      |
| `compile_flags` | Optimization / debug flags.                                                                                           | `["-O3"]`                                                          |
| `link`          | Link command to use. Should alwas be `cc`.                                                                            | `cc`                                                               |
| `link_args`     | Arguments to pass to `link` to link the grammar.                                                                      | `["-shared", "-fpic", "parser.o", "-o", "{lang}.so"]` |
| `link_flags`    | Optimization / debug / additional libraries to link flags.                                                            | `["-O3"]`                                                          |

## `language`

The `language` table contains language-keyed configuration — e.g. `language.rust`.

| Field                        | Description                                                                                                               | Default |
| -----                        | -----------                                                                                                               | ------- |
| `remove_default_highlighter` | For removing the default highlighter set by the Kakoune distribution when enabling `kak-tree-sitter` support in a buffer. | `true`  |
| `filetype_hook`              | For activating a per-language that forwards the value of theKakoune `filetype` option to `tree_sitter_lang`.              | `true`  |
| `aliases`                    | A list of alternate language names (useful for when your language can have several names, like `bash`, `sh`, etc.).       |         |
| `grammar`                    | For linking with a grammar.                                                                                               |         |
| `queries`                    | For defining queries.                                                                                                     |         |

### `language.<lang>.remove_default_higlighter`

Remove the default highlighter set by the Kakoune _“standard library”_ (i.e.
`window/<lang>`). For instance, for the `rust` `filetype`, the default highlighter
is `window/rust`. Setting this option to `true` will remove this highlighter, which
is almost always wanted (otherwise, the highlighting from KTS might not be
applied properly).

Some languages might have an incomplete tree-sitter support; in such a case, you
might not want to remove the default highlighter. Set this option to `false` in
such cases, then.

### `language.<lang>.filetype_hook`

Install a hook for `<lang>` that will forward the content of `filetype` into
`tree_sitter_lang`. This is highly recommended for most users, so you should not
have to tweak that default value.

### `language.<lang>.aliases`

List of language names that can be used in place of `<lang>`.

### `language.<lang>.grammar`

A string representing a grammar name. Defining this will use the associated
grammar instead of `<lang>`.

### `language.<lang>.queries`

This section provides the required data to know how to fetch queries.

The following field(s) are mandatory:

| Field    | Description                                                                                                                                                             |
| -----    | -----------                                                                                                                                                             |
| `source` | Optional source from where to pick the queries; see the [Sources](#sources) section. If you omit it, the same `source` object is used for both the grammar and queries. |

The following fields are optional and you should only provide the ones you need:

| Field  | Description                                                 | Default                  |
| -----  | -----------                                                 | -------                  |
| `path` | Path where to find the queries (the `.scm` files) directory | `runtime/queries/{lang}` |

# Sources

Sources are a way to provide information from where runtime resources come from.
We currently support two sources:

- Local paths (`local.path`).
- Git repositories (`git`), which is an object containing the following fields:
  - `url`: the URL to fetch from. Will use `git clone`.
  - `pin`: _pin ref_, such as a commit, branch name or tag.

If you decide to use a `git` source:

- Grammars must be _fetched_, _compiled_ and _installed_. `ktsctl` can do that
  automatically for you, provided you have the right configuration, by using
  the appropriate commands. See the documentation of [ktsctl](ktsctl.md).
- Queries must be _fetched_ and _installed_, the same way as with grammars.
- When you decide to install a _“language”_, both the grammars and queries might
  be fetched, compiled and installed if the configuration requires both to be.
  Hence, a single CLI command should basically do everything for you — `ktsctl sync`.

If you decide to use a `local` source, **`ktsctl` will do nothing for you** and
will simply display a message explaining that it will use a path. Nothing will
be fetched, compiled nor installed. **It’s up to you to do so.**

For users installing `ktsctl` by using a binary release or compiling it
themselves, the default configuration (which uses `git` sources) is enough.
However, if you ship with a distributed set of grammars and queries, you might
want to override the languages’ configurations and use `local` sources. You can
also mix them: a `git` source for the grammar, and a `local` one for the
queries. It’s up to you.

# Next

You can have a look at [ktsctl] and [features] to start exploring what you can do!

[ktsctl]: ktsctl.md
[features]: features.md
