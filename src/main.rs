use asset_decrypter::{DEFAULT_KEY, Decrypter};
use clap::{Parser, Subcommand, crate_version, value_parser};
use std::{
    fs::{read, read_dir, read_to_string, write},
    path::PathBuf,
    process::exit,
};

#[derive(Parser)]
#[command(version = crate_version!(), about = "Decrypt/encrypt RPG Maker MV/MZ audio and image assets.", next_line_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    /// Encryption key for encrypt/decrypt operations
    #[arg(short = 'e', long, global = true)]
    key: Option<String>,
    /// Game engine
    #[arg(short = 'E', long, value_parser = ["mv", "mz"], global = true)]
    engine: Option<String>,
    /// Input directory
    #[arg(short, long, default_value = "./", value_parser = value_parser!(PathBuf), hide_default_value = true, global = true)]
    input_dir: PathBuf,
    /// Output directory
    #[arg(short, long, default_value = "./", value_parser = value_parser!(PathBuf), hide_default_value = true, global = true)]
    output_dir: PathBuf,
    /// File path (for single file processing or key extraction)
    #[arg(short, long, value_parser = value_parser!(PathBuf), global = true)]
    file: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Decrypts encrypted assets.
    /// .rpgmvo/.ogg_ => .ogg
    /// .rpgmvp/.png_ => .png
    /// .rpgmvm/.m4a_ => .m4a
    Encrypt,
    /// Encrypts .png/.ogg/m4a assets.
    /// .ogg => .rpgmvo/.ogg_
    /// .png => .rpgmvp/.png_
    /// .m4a => .rpgmvm/.m4a_
    Decrypt,
    /// Extracts key from file, specified in --file argument.
    ExtractKey,
}

fn main() {
    let cli: Cli = Cli::parse();
    let engine: String = cli.engine.unwrap_or_else(|| {
        eprintln!("--engine argument is not specified.");
        exit(1);
    });

    let mut decrypter: Decrypter = if cli.key.is_none() && matches!(cli.command, Commands::Encrypt)
    {
        Decrypter::new(Some(String::from(DEFAULT_KEY)))
    } else {
        Decrypter::new(cli.key)
    };

    match cli.command {
        Commands::ExtractKey => {
            let file: PathBuf = cli.file.unwrap_or_else(|| {
                eprintln!("--file argument is not specified.");
                exit(1);
            });
            let key: String = if file.extension().unwrap() == "json" {
                let content: String = read_to_string(&file).unwrap();
                let index: usize =
                    content.rfind("encryptionKey").unwrap() + "encryptionKey\":".len();
                content[index..].trim().trim_matches('"')[..32].to_string()
            } else {
                let buf: Vec<u8> = read(&file).unwrap();
                decrypter.set_key_from_image(&buf);
                decrypter.key()
            };
            println!("Encryption key: {key}");
        }
        Commands::Decrypt | Commands::Encrypt => {
            let mut process_file = |file: &PathBuf| {
                let data: Vec<u8> = read(file).unwrap();
                let (processed, new_ext) = match cli.command {
                    Commands::Decrypt => {
                        let decrypted: Vec<u8> = decrypter.decrypt(&data);
                        let ext: &str = file.extension().unwrap().to_str().unwrap();
                        let new_ext: &str = match ext {
                            "rpgmvp" | "png_" => "png",
                            "rpgmvo" | "ogg_" => "ogg",
                            "rpgmvm" | "m4a_" => "m4a",
                            _ => unreachable!(),
                        };
                        (decrypted, new_ext)
                    }
                    Commands::Encrypt => {
                        let encrypted: Vec<u8> = decrypter.encrypt(&data);
                        let ext: &str = file.extension().unwrap().to_str().unwrap();
                        let new_ext: &str = match (engine.as_str(), ext) {
                            ("mv", "png") => "rpgmvp",
                            ("mv", "ogg") => "rpgmvo",
                            ("mv", "m4a") => "rpgmvm",
                            ("mz", "png") => "png_",
                            ("mz", "ogg") => "ogg_",
                            ("mz", "m4a") => "m4a_",
                            _ => unreachable!(),
                        };
                        (encrypted, new_ext)
                    }
                    _ => unreachable!(),
                };
                let output_file: PathBuf = cli
                    .output_dir
                    .join(PathBuf::from(file.file_name().unwrap()).with_extension(new_ext));
                write(output_file, processed).unwrap();
            };

            if let Some(file) = &cli.file {
                process_file(file);
            } else {
                let exts: &[&str] = match cli.command {
                    Commands::Encrypt => &["png", "ogg", "m4a"],
                    Commands::Decrypt => &["rpgmvp", "rpgmvo", "rpgmvm", "ogg_", "png_", "m4a_"],
                    _ => unreachable!(),
                };
                for entry in read_dir(&cli.input_dir).unwrap().flatten() {
                    let path: PathBuf = entry.path();
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if exts.contains(&ext) {
                            process_file(&path);
                        }
                    }
                }
            }
        }
    }
}
