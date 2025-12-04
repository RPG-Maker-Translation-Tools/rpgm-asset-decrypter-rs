#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::needless_doctest_main)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::deref_addrof)]

use anyhow::{Result, bail};
use asset_decrypter::{Decrypter, FileType, HEADER_LENGTH, RPGM_HEADER};
use clap::{Parser, Subcommand, ValueEnum, value_parser};
use serde_json::{Value, from_str};
use std::{
    ffi::OsStr,
    fs::{read, read_dir, read_to_string, write},
    path::{Path, PathBuf},
    time::Instant,
};
use strum_macros::EnumIs;

#[derive(Debug, Copy, Clone, ValueEnum)]
pub enum Engine {
    MV,
    MZ,
}

#[derive(Parser)]
#[command(
    about = "Decrypt/encrypt RPG Maker MV/MZ audio and image assets.",
    version,
    next_line_help = true,
    term_width = 120
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    /// Encryption key for encryption/decryption. Decrypt command automatically finds the key from processed files, so you probably don't need to set it when decrypting.
    #[arg(short = 'e', long, global = true)]
    key: Option<String>,
    /// Game engine - `mv` or `mz`. Required for encryption
    #[arg(short = 'E', long, global = true, value_parser = ["mv", "mz"])]
    engine: Option<Engine>,
    /// Input directory
    #[arg(short, long, default_value = "./", value_parser = value_parser!(PathBuf), hide_default_value = true, global = true)]
    input_dir: PathBuf,
    /// Output directory
    #[arg(short, long, value_parser = value_parser!(PathBuf), hide_default_value = true, global = true)]
    output_dir: Option<PathBuf>,
    /// File path (for single file processing or key extraction)
    #[arg(short, long, value_parser = value_parser!(PathBuf), global = true, conflicts_with = "input_dir")]
    file: Option<PathBuf>,
}

#[derive(Subcommand, EnumIs, Clone, Copy)]
enum Command {
    /// Encrypts .png/.ogg/.m4a assets. Requires `--engine` and `--key` arguments to be set
    ///
    /// .ogg => .rpgmvo/.ogg_
    ///
    /// .png => .rpgmvp/.png_
    ///
    /// .m4a => .rpgmvm/.m4a_
    Encrypt,

    /// Decrypts encrypted assets. Automatically deduces the key for each processed file
    ///
    /// .rpgmvo/.ogg_ => .ogg
    ///
    /// .rpgmvp/.png_ => .png
    ///
    /// .rpgmvm/.m4a_ => .m4a
    Decrypt,

    /// Extracts key from file, specified in --file argument. Key can only be extracted from System.json file or RPG Maker encrypted file.
    ExtractKey,
}

const MV_PNG_EXT: &str = "rpgmvp";
const MV_OGG_EXT: &str = "rpgmvo";
const MV_M4A_EXT: &str = "rpgmvm";

const MZ_PNG_EXT: &str = "png_";
const MZ_OGG_EXT: &str = "ogg_";
const MZ_M4A_EXT: &str = "m4a_";

const PNG_EXT: &str = "png";
const OGG_EXT: &str = "ogg";
const M4A_EXT: &str = "m4a";

const DECRYPT_EXTENSIONS: &[&str] = &[
    MV_PNG_EXT, MV_OGG_EXT, MV_M4A_EXT, MZ_PNG_EXT, MZ_OGG_EXT, MZ_M4A_EXT,
];
const ENCRYPT_EXTENSIONS: &[&str] = &[PNG_EXT, OGG_EXT, M4A_EXT];

struct Processor<'a> {
    decrypter: Decrypter,
    command: Command,
    engine: Engine,
    output_dir: &'a Path,
    input_dir: &'a Path,
    file: Option<&'a PathBuf>,
    global_key_set: bool,
}

impl<'a> Processor<'a> {
    pub fn new(cli: &'a Cli) -> Result<Self, anyhow::Error> {
        let mut decrypter = Decrypter::new();
        let mut engine = Engine::MV;

        if let Some(file) = &cli.file {
            if !file.is_file() {
                bail!("--file argument expects file as its argument.");
            }
        } else if cli.command.is_extract_key() {
            bail!("--file argument is not specified.");
        }

        if let Some(key) = &cli.key {
            decrypter.set_key_from_str(key)?;
        } else if cli.command.is_encrypt() {
            bail!("--key argument is not specified.");
        }

        if let Some(eng) = cli.engine {
            engine = eng;
        } else if cli.command.is_encrypt() {
            bail!("--engine argument is not specified.");
        }

        let output_dir = cli.output_dir.as_ref().unwrap_or(&cli.input_dir);

        Ok(Self {
            decrypter,
            command: cli.command,
            engine,
            output_dir,
            input_dir: &cli.input_dir,
            file: cli.file.as_ref(),
            global_key_set: cli.key.is_some(),
        })
    }

    fn process_file(
        &mut self,
        file: &Path,
        extension: &str,
    ) -> Result<(), anyhow::Error> {
        let mut file_data = read(file)?;

        let new_extension = if self.command.is_decrypt() {
            let file_type = FileType::try_from(extension).unwrap();

            // This is unlikely, but if we processing a directory when files have different encryption keys, we need to always reset the key
            if !self.global_key_set {
                self.decrypter.set_key_from_file(&file_data, file_type)?;
            }

            let sliced =
                self.decrypter.decrypt_in_place(&mut file_data, file_type)?;

            match extension {
                MV_PNG_EXT | MZ_PNG_EXT => {
                    if !sliced.starts_with(b"\x89PNG\r\n\x1a\n") {
                        bail!(
                            "Decrypted PNG file has invalid signature. Check if you supplied correct key in `--key` argument."
                        );
                    }
                }
                MV_OGG_EXT | MZ_OGG_EXT => {
                    const OGG_SIGNATURE: &[u8] = b"OggS";
                    if !sliced.starts_with(OGG_SIGNATURE) {
                        bail!(
                            "Decrypted OGG file has invalid signature. Check if you supplied correct key in `--key` argument."
                        );
                    }
                }
                MV_M4A_EXT | MZ_M4A_EXT => {
                    if sliced.len() < 12 || &sliced[4..8] != b"ftyp" {
                        bail!(
                            "Decrypted M4A file has invalid signature. Check if you supplied correct key in `--key` argument."
                        );
                    }
                }
                _ => unreachable!(),
            }

            match extension {
                MV_PNG_EXT | MZ_PNG_EXT => PNG_EXT,
                MV_OGG_EXT | MZ_OGG_EXT => OGG_EXT,
                MV_M4A_EXT | MZ_M4A_EXT => M4A_EXT,
                _ => unreachable!(),
            }
        } else {
            self.decrypter.encrypt_in_place(&mut file_data)?;

            match (self.engine, extension) {
                (Engine::MV, PNG_EXT) => MV_PNG_EXT,
                (Engine::MV, OGG_EXT) => MV_OGG_EXT,
                (Engine::MV, M4A_EXT) => MV_M4A_EXT,
                (Engine::MZ, PNG_EXT) => MZ_PNG_EXT,
                (Engine::MZ, OGG_EXT) => MZ_OGG_EXT,
                (Engine::MZ, M4A_EXT) => MZ_M4A_EXT,
                _ => unreachable!(),
            }
        };

        let output_file_name =
            PathBuf::from(unsafe { file.file_name().unwrap_unchecked() })
                .with_extension(new_extension);

        let output_file_path = self.output_dir.join(output_file_name);

        if self.command.is_decrypt() {
            write(output_file_path, &file_data[HEADER_LENGTH..])?;
        } else {
            let mut output_data =
                Vec::with_capacity(RPGM_HEADER.len() + file_data.len());
            output_data.extend(RPGM_HEADER);
            output_data.extend(file_data);

            write(output_file_path, output_data)?;
        }

        Ok(())
    }

    pub fn extract_key(&mut self) -> Result<(), anyhow::Error> {
        let file_path = unsafe { self.file.unwrap_unchecked() };
        let extension = unsafe {
            file_path
                .extension()
                .and_then(OsStr::to_str)
                .unwrap_unchecked()
        };
        let filename = unsafe { file_path.file_name().unwrap_unchecked() };
        let system_value: Value;

        let key = if filename == "System.json" {
            let system_file_content = read_to_string(file_path)?;

            system_value = from_str(&system_file_content)?;
            system_value["encryptionKey"].as_str().unwrap()
        } else if DECRYPT_EXTENSIONS.contains(&extension) {
            let file_data = read(file_path)?;
            self.decrypter.set_key_from_file(
                &file_data,
                FileType::try_from(extension).unwrap(),
            )?
        } else {
            bail!(
                "Key can be extracted only from `System.json` file or RPG Maker encrypted file."
            );
        };

        println!("Encryption key: {key}");
        Ok(())
    }

    pub fn process(&mut self) -> Result<(), anyhow::Error> {
        if self.command.is_extract_key() {
            self.extract_key()?;
        } else {
            let allowed_extensions = if self.command.is_encrypt() {
                ENCRYPT_EXTENSIONS
            } else {
                DECRYPT_EXTENSIONS
            };

            if let Some(file) = &self.file {
                if let Some(extension) =
                    file.extension().and_then(OsStr::to_str)
                    && allowed_extensions.contains(&extension)
                {
                    self.process_file(file, extension)?;
                }
            } else {
                for entry in read_dir(self.input_dir)?.flatten() {
                    let path = entry.path();

                    if let Some(extension) =
                        path.extension().and_then(OsStr::to_str)
                        && allowed_extensions.contains(&extension)
                    {
                        self.process_file(&path, extension)?;
                    }
                }
            }
        }

        Ok(())
    }
}

fn main() -> Result<()> {
    let start_time = Instant::now();

    let cli = Cli::parse();
    let mut processor = Processor::new(&cli)?;
    processor.process()?;

    println!("Elapsed: {:.2}s", start_time.elapsed().as_secs_f32());
    Ok(())
}
