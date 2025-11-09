use anyhow::{Context, Result, bail};
use asset_decrypter::{DEFAULT_KEY, Decrypter};
use clap::{Parser, Subcommand, ValueEnum, value_parser};
use serde_json::{Value, from_str};
use std::{
    fs::{read, read_dir, read_to_string, write},
    path::PathBuf,
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
    command: Commands,
    /// Encryption key for encrypt/decrypt operations
    #[arg(short = 'e', long, global = true)]
    key: Option<String>,
    /// Game engine
    #[arg(short = 'E', long, global = true)]
    engine: Option<Engine>,
    /// Input directory
    #[arg(short, long, default_value = "./", value_parser = value_parser!(PathBuf), hide_default_value = true, global = true)]
    input_dir: PathBuf,
    /// Output directory
    #[arg(short, long, value_parser = value_parser!(PathBuf), hide_default_value = true, global = true)]
    output_dir: Option<PathBuf>,
    /// File path (for single file processing or key extraction)
    #[arg(short, long, value_parser = value_parser!(PathBuf), global = true)]
    file: Option<PathBuf>,
}

#[derive(Subcommand, EnumIs)]
enum Commands {
    /// Encrypts .png/.ogg/.m4a assets. Requires `--engine` argument to be set
    ///
    /// .ogg => .rpgmvo/.ogg_
    ///
    /// .png => .rpgmvp/.png_
    ///
    /// .m4a => .rpgmvm/.m4a_
    Encrypt,
    /// Decrypts encrypted assets.
    ///
    /// .rpgmvo/.ogg_ => .ogg
    ///
    /// .rpgmvp/.png_ => .png
    ///
    /// .rpgmvm/.m4a_ => .m4a
    Decrypt,
    /// Extracts key from file, specified in --file argument. Key can only be extracted from System.json, .rpgmvp and .png_ files.
    ExtractKey,
}

const DECRYPT_EXTENSIONS: &[&str] =
    &["rpgmvp", "rpgmvo", "rpgmvm", "ogg_", "png_", "m4a_"];
const ENCRYPT_EXTENSIONS: &[&str] = &["png", "ogg", "m4a"];

fn main() -> Result<()> {
    let start_time = Instant::now();
    let cli = Cli::parse();

    let mut decrypter = Decrypter::new();

    if cli.command.is_decrypt() {
        if let Some(key) = &cli.key {
            unsafe {
                decrypter.set_key_from_str(key).unwrap_unchecked();
            }
        } else {
            println!(
                "--key argument is not specified. Using default key - decrypted files may not be valid."
            );
            unsafe {
                decrypter.set_key_from_str(DEFAULT_KEY).unwrap_unchecked();
            }
        }
    } else if cli.command.is_encrypt() {
        if let Some(key) = &cli.key {
            decrypter.set_key_from_str(key)?;
        } else {
            bail!("--key argument is not specified.");
        }

        if cli.engine.is_none() {
            bail!("--engine argument is not specified.");
        }
    };

    if cli.command.is_extract_key() {
        let file_path =
            cli.file.context("--file argument is not specified.")?;
        let extension = file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .context("--file argument expects file as its value.")?;
        let filename = unsafe { file_path.file_name().unwrap_unchecked() };
        let system_value: Value;

        let key = if filename == "System.json" {
            let system_file_content = read_to_string(&file_path)?;

            system_value =
                unsafe { from_str(&system_file_content).unwrap_unchecked() };
            unsafe { system_value["encryptionKey"].as_str().unwrap_unchecked() }
        } else if ["rpgmvp", "png_"].contains(&extension) {
            let file_data = read(&file_path)?;
            decrypter.set_key_from_image(&file_data);
            unsafe { decrypter.key().unwrap_unchecked() }
        } else {
            bail!(
                "Key can be extracted only from `System.json` file or `.rpgmvp`/`.png_` file."
            );
        };

        println!("Encryption key: {key}");
    } else {
        let output_dir =
            cli.output_dir.unwrap_or_else(|| cli.input_dir.clone());

        let mut process_file = |file: &PathBuf,
                                extension: &str|
         -> Result<()> {
            let file_data = read(file)?;

            let (processed, new_extension) = if cli.command.is_decrypt() {
                let decrypted = decrypter.decrypt(&file_data);
                let key = unsafe { cli.key.as_ref().unwrap_unchecked() };

                match extension {
                    "rpgmvp" | "png_" => {
                        // PNG: 89 50 4E 47 0D 0A 1A 0A
                        let png_signature =
                            &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
                        if decrypted.len() < png_signature.len()
                            || &decrypted[..8] != png_signature
                        {
                            bail!(
                                "Decrypted PNG file has invalid signature. {}",
                                if key == DEFAULT_KEY {
                                    "Instead of using default key, extract key from game files using `extract-key` command, and then supply it using `--key` argument in decryption."
                                } else {
                                    "Check if you supplied correct key in `--key` argument."
                                }
                            );
                        }
                    }
                    "rpgmvo" | "ogg_" => {
                        // OGG: 4F 67 67 53
                        let ogg_signature = b"OggS";
                        if decrypted.len() < ogg_signature.len()
                            || &decrypted[..4] != ogg_signature
                        {
                            bail!(
                                "Decrypted OGG file has invalid signature. {}",
                                if key == DEFAULT_KEY {
                                    "Instead of using default key, extract key from game files using `extract-key` command, and then supply it using `--key` argument in decryption."
                                } else {
                                    "Check if you supplied correct key in `--key` argument."
                                }
                            );
                        }
                    }
                    "rpgmvm" | "m4a_" => {
                        // M4A: 00 00 00 ?? 66 74 79 70 4D 34 41 20 (ftypM4A)
                        if decrypted.len() < 12
                            || &decrypted[4..8] != b"ftyp"
                            || &decrypted[8..12] != b"M4A "
                        {
                            bail!(
                                "Decrypted M4A file has invalid signature. {}",
                                if key == DEFAULT_KEY {
                                    "Instead of using default key, extract key from game files using `extract-key` command, and then supply it using `--key` argument in decryption."
                                } else {
                                    "Check if you supplied correct key in `--key` argument."
                                }
                            );
                        }
                    }
                    _ => unreachable!(),
                }

                let new_extension = match extension {
                    "rpgmvp" | "png_" => "png",
                    "rpgmvo" | "ogg_" => "ogg",
                    "rpgmvm" | "m4a_" => "m4a",
                    _ => unreachable!(),
                };

                (decrypted, new_extension)
            } else {
                let encrypted = decrypter.encrypt(&file_data)?;
                let engine = unsafe { cli.engine.unwrap_unchecked() };

                let new_extension = match (engine, extension) {
                    (Engine::MV, "png") => "rpgmvp",
                    (Engine::MV, "ogg") => "rpgmvo",
                    (Engine::MV, "m4a") => "rpgmvm",
                    (Engine::MZ, "png") => "png_",
                    (Engine::MZ, "ogg") => "ogg_",
                    (Engine::MZ, "m4a") => "m4a_",
                    _ => unreachable!(),
                };

                (encrypted, new_extension)
            };

            let output_file_name =
                PathBuf::from(unsafe { file.file_name().unwrap_unchecked() })
                    .with_extension(new_extension);

            let output_file_path = output_dir.join(output_file_name);
            write(output_file_path, processed)?;
            Ok(())
        };

        let allowed_extensions = if cli.command.is_encrypt() {
            ENCRYPT_EXTENSIONS
        } else {
            DECRYPT_EXTENSIONS
        };

        if let Some(file) = &cli.file {
            if let Some(extension) = file.extension().and_then(|e| e.to_str())
                && allowed_extensions.contains(&extension)
            {
                process_file(file, extension)?;
            }
        } else {
            for entry in read_dir(&cli.input_dir)?.flatten() {
                let path = entry.path();

                if let Some(extension) =
                    path.extension().and_then(|e| e.to_str())
                    && allowed_extensions.contains(&extension)
                {
                    process_file(&path, extension)?;
                }
            }
        }
    }

    println!("Elapsed: {:.2}s", start_time.elapsed().as_secs_f32());
    Ok(())
}
