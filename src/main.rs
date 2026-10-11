#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
#![allow(clippy::cast_possible_truncation)]

use anyhow::{Context, Result, bail};
use asset_decrypter::{
    DECRYPTED_ASSETS_EXTS, Decrypter, ENCRYPTED_ASSET_EXTS, FileType, HEADER_LENGTH, M4A_EXT, MV_M4A_EXT, MV_OGG_EXT,
    MV_PNG_EXT, MZ_M4A_EXT, MZ_OGG_EXT, MZ_PNG_EXT, OGG_EXT, PNG_EXT, RPGM_HEADER,
};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, from_str};
use std::{
    fs::{File, create_dir_all, read, read_dir, read_to_string, write},
    io::Read,
    path::{Path, PathBuf},
    time::Instant,
};

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
    #[arg(short = 'E', long, global = true)]
    engine: Option<Engine>,
    /// Input directory. Not recursive: point it at the directory that directly contains the assets, e.g. `www/img/tilesets`
    #[arg(short, long, default_value = "./", hide_default_value = true, global = true)]
    input_dir: PathBuf,
    /// Output directory (created if missing). Defaults to the input directory, or to the file's directory when `--file` is used
    #[arg(short, long, global = true)]
    output_dir: Option<PathBuf>,
    /// File path (for single file processing or key extraction)
    #[arg(short, long, global = true, conflicts_with = "input_dir")]
    file: Option<PathBuf>,
}

#[derive(Subcommand, Clone, Copy, PartialEq, Eq)]
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

/// Returns the entry of `allowed` equal to `path`'s extension, ignoring case.
/// The returned value is the canonical lowercase spelling, so it can be matched against the extension constants.
fn matching_extension(path: &Path, allowed: &[&'static str]) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?;
    allowed.iter().copied().find(|candidate| candidate.eq_ignore_ascii_case(extension))
}

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
    pub fn new(cli: &'a Cli) -> Result<Self> {
        let mut decrypter = Decrypter::new();
        let mut engine = Engine::MV;

        if let Some(file) = &cli.file {
            if !file.is_file() {
                bail!("--file argument expects file as its argument.");
            }
        } else if cli.command == Command::ExtractKey {
            bail!("--file argument is not specified.");
        }

        if let Some(key) = &cli.key {
            decrypter.set_key_from_str(key)?;
        } else if cli.command == Command::Encrypt {
            bail!("--key argument is not specified.");
        }

        if let Some(eng) = cli.engine {
            engine = eng;
        } else if cli.command == Command::Encrypt {
            bail!("--engine argument is not specified.");
        }

        let output_dir = match (&cli.output_dir, &cli.file) {
            (Some(dir), _) => dir.as_path(),
            // A bare file name has an empty parent, which is not a usable directory.
            (None, Some(file)) => file
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
            (None, None) => cli.input_dir.as_path(),
        };

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

    /// `extension` must be the canonical spelling returned by [`matching_extension`].
    fn process_file(&mut self, file: &Path, extension: &str) -> Result<()> {
        // `Path::extension` is `Some` only if `Path::file_name` is, and callers only pass paths with an extension.
        let file_name = unsafe { file.file_name().unwrap_unchecked() };

        if self.command == Command::Decrypt {
            // `extension` is one of `ENCRYPTED_ASSET_EXTS`, which all have a file type.
            let file_type = FileType::try_from(extension).unwrap();
            let mut file_data = read(file)?;

            // This is unlikely, but if we processing a directory when files have different encryption keys, we need to always reset the key
            if !self.global_key_set {
                self.decrypter.set_key_from_file(&file_data, file_type)?;
            }

            let decrypted = self.decrypter.decrypt_in_place(&mut file_data, file_type)?;

            let valid = match file_type {
                FileType::PNG => decrypted.starts_with(b"\x89PNG\r\n\x1a\n"),
                FileType::OGG => decrypted.starts_with(b"OggS"),
                FileType::M4A => decrypted.get(4..8).is_some_and(|tag| tag == b"ftyp"),
            };

            if !valid {
                bail!("Decrypted {file_type} file has invalid signature. Check if you supplied correct key in `--key` argument.");
            }

            let new_extension = match file_type {
                FileType::PNG => PNG_EXT,
                FileType::OGG => OGG_EXT,
                FileType::M4A => M4A_EXT,
            };

            write(self.output_dir.join(file_name).with_extension(new_extension), decrypted)?;
        } else {
            let new_extension = match (self.engine, extension) {
                (Engine::MV, PNG_EXT) => MV_PNG_EXT,
                (Engine::MV, OGG_EXT) => MV_OGG_EXT,
                (Engine::MV, M4A_EXT) => MV_M4A_EXT,
                (Engine::MZ, PNG_EXT) => MZ_PNG_EXT,
                (Engine::MZ, OGG_EXT) => MZ_OGG_EXT,
                (Engine::MZ, M4A_EXT) => MZ_M4A_EXT,
                _ => unreachable!(),
            };

            // Read straight behind the header, so the file is neither copied nor reallocated.
            let mut input = File::open(file)?;
            let mut output_data = Vec::with_capacity(HEADER_LENGTH + input.metadata()?.len() as usize);
            output_data.extend_from_slice(RPGM_HEADER);
            input.read_to_end(&mut output_data)?;

            self.decrypter.encrypt_in_place(&mut output_data[HEADER_LENGTH..])?;
            write(self.output_dir.join(file_name).with_extension(new_extension), output_data)?;
        }

        Ok(())
    }

    pub fn extract_key(&mut self) -> Result<()> {
        let Some(file_path) = self.file else {
            bail!("--file argument is not specified.");
        };

        if file_path.file_name().is_some_and(|name| name.eq_ignore_ascii_case("System.json")) {
            let content = read_to_string(file_path)?;
            // Some editors prepend a BOM, which serde_json rejects.
            let system: Value = from_str(content.trim_start_matches('\u{feff}'))?;

            let Some(key) = system["encryptionKey"].as_str() else {
                bail!("`encryptionKey` not found in System.json. The game is probably not encrypted.");
            };

            println!("Encryption key: {key}");
        } else if let Some(extension) = matching_extension(file_path, ENCRYPTED_ASSET_EXTS) {
            let file_data = read(file_path)?;
            // `extension` is one of `ENCRYPTED_ASSET_EXTS`, which all have a file type.
            let key = self.decrypter.set_key_from_file(&file_data, FileType::try_from(extension).unwrap())?;
            println!("Encryption key: {key}");
        } else {
            bail!("Key can be extracted only from `System.json` file or RPG Maker encrypted file.");
        }

        Ok(())
    }

    pub fn process(&mut self) -> Result<()> {
        if self.command == Command::ExtractKey {
            return self.extract_key();
        }

        let allowed_extensions = if self.command == Command::Encrypt {
            DECRYPTED_ASSETS_EXTS
        } else {
            ENCRYPTED_ASSET_EXTS
        };

        if let Some(file) = self.file {
            let Some(extension) = matching_extension(file, allowed_extensions) else {
                bail!("{} has unsupported extension.", file.display());
            };

            create_dir_all(self.output_dir)?;
            self.process_file(file, extension)
                .with_context(|| file.display().to_string())?;
            println!("Processed 1 file.");
        } else {
            create_dir_all(self.output_dir)?;

            let mut processed: usize = 0;
            let mut failed: usize = 0;

            for entry in read_dir(self.input_dir)?.flatten() {
                let path = entry.path();
                let Some(extension) = matching_extension(&path, allowed_extensions) else {
                    continue;
                };

                // A single broken file shouldn't throw away the rest of the batch.
                match self.process_file(&path, extension) {
                    Ok(()) => processed += 1,
                    Err(error) => {
                        eprintln!("{}: {error:#}", path.display());
                        failed += 1;
                    }
                }
            }

            println!("Processed {processed} file(s).");

            if failed > 0 {
                bail!("{failed} file(s) failed to process.");
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
