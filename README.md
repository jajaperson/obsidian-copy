# Obsidian Copy

_Obsidan Copy is a CLI program and Rust library to copy part of an [Obsidian](https://obsidian.md)
vault to an external directory_

Parts of this project are based on the distinct
[obsidian-export](https://github.com/zoni/obsidian-export), which exports some or all of a vault to
[CommonMark](https://commonmark.org).

## Rationale

When it comes to Obsidian, I'm in favour of the monolithic vault approach. Using a single vault for
everything facilitates richer link-making acros disciplines and topics. The only drawback of this
approach is sharing one's vault becomes exceedingly difficult: Mathematical results are jumbled
with recipes and diary entries I'd rather the world didn't see. Obsidian Copy aims to solve this
problem, by selectively copying a part of a vault based on filters, such as tags.

## How it works

I'm not ready to release this yet and the functionality is subject to change. Currently this only
supports filtering by tags for `.md` files. Any other files are included iff they are linked to (or
embedded) by an included `.md` file. Basic usage is as follows

```
obsidian-export --root Vault -destination Export --include-tags public --exclude-tags private
```
