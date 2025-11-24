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

# You can extract encryption key from any encrypted file
rpgmasd extract-key --file image.rpgmvp

# `encrypt` command requires `--engine` and `--key` arguments
rpgmasd encrypt --engine mv --key d41d8cd98f00b204e9800998ecf8427e -i "./images"
```

## License

Project is licensed under WTFPL.
