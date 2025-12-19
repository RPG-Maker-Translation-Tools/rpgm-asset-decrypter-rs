# rpgm-asset-decrypter-rs

A CLI tool for decrypting/encrypting RPG Maker MV/MZ audio and image assets.

Supports `rpgmvp`/`png_`, `rpgmvo`/`ogg_`, `rpgmvm`/`m4a_` assets.

Built on top of blazingly fast [rpgm-asset-decrypter-lib](https://github.com/RPG-Maker-Translation-Tools/rpgm-asset-decrypter-lib).

## Installation

Get the binaries in Releases section.

## Usage

```bash
# Decrypt all files in a directory. You don't need to set a key for this - program will automatically extract it from processed files
rpgmasd decrypt -i "./rpg-maker-mv-game/www/img/tilesets"

# Decrypt a single file in a directory
rpgmasd decrypt --file image.rpgmvp
rpgmasd decrypt --file audio.rpgmvo
rpgmasd decrypt --file audio.m4a_

# You can extract encryption key from any encrypted file
rpgmasd extract-key --file image.rpgmvp

# `encrypt` command requires `--engine` and `--key` arguments
rpgmasd encrypt --engine mv --key d41d8cd98f00b204e9800998ecf8427e -i "./images"
```

## GUI

Our [rpgmdec](https://github.com/rpg-maker-translation-tools/rpgmdec) GUI provides the same functionality as `rpgmasd`.

## Building

Requirements: `rustup` with installed Rust toolchain.

Clone the repository and compile with `cargo b -r`.

## Support

[Me](https://github.com/savannstm), the maintainer of this project, is a poor college student from Eastern Europe.

If you could, please consider supporting us through:

-   [Ko-fi](https://ko-fi.com/savannstm)
-   [Patreon](https://www.patreon.com/cw/savannstm)
-   [Boosty](https://boosty.to/mcdeimos)

Even if you don't, it's fine. We'll continue to do as we right now.

## License

Project is licensed under WTFPL.
