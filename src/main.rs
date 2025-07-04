use asset_decrypter::{DEFAULT_KEY, Decrypter, KEY_LENGTH};
use clap::{Parser, Subcommand, value_parser};
use std::{
    fs::{read, read_dir, read_to_string, write},
    path::PathBuf,
    process::exit,
    time::Instant,
};

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
    #[arg(short = 'E', long, value_parser = ["mv", "mz"], global = true)]
    engine: Option<String>,
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

#[derive(Subcommand)]
enum Commands {
    /// Decrypts encrypted assets.
    ///
    /// .rpgmvo/.ogg_ => .ogg
    ///
    /// .rpgmvp/.png_ => .png
    ///
    /// .rpgmvm/.m4a_ => .m4a
    Encrypt,
    /// Encrypts .png/.ogg/m4a assets.
    ///
    /// .ogg => .rpgmvo/.ogg_
    ///
    /// .png => .rpgmvp/.png_
    ///
    /// .m4a => .rpgmvm/.m4a_
    Decrypt,
    /// Extracts key from file, specified in --file argument.
    ExtractKey,
}

fn main() {
    let start_time = Instant::now();
    let cli: Cli = Cli::parse();
    let engine: String = cli.engine.unwrap_or_else(|| {
        eprintln!("--engine argument is not specified.");
        exit(1);
    });

    let mut decrypter: Decrypter = Decrypter::new();

    if cli.key.is_none() && matches!(cli.command, Commands::Encrypt) {
        decrypter.set_key_from_str(DEFAULT_KEY).unwrap()
    } else {
        decrypter
            .set_key_from_str(cli.key.unwrap().as_str())
            .unwrap()
    };

    match cli.command {
        Commands::ExtractKey => {
            let file_path: PathBuf = cli.file.unwrap_or_else(|| {
                eprintln!("--file argument is not specified.");
                exit(1);
            });

            let file_content: String;

            let key = if file_path.extension().unwrap() == "json" {
                file_content = read_to_string(&file_path).unwrap();
                let encryption_key_index: usize =
                    file_content.rfind("encryptionKey").unwrap() + "encryptionKey\":".len();
                &file_content[encryption_key_index..]
                    .trim()
                    .trim_matches('"')[..KEY_LENGTH]
            } else {
                let file_data: Vec<u8> = read(&file_path).unwrap();
                decrypter.set_key_from_image(&file_data);
                decrypter.key().unwrap()
            };

            println!("Encryption key: {key}");
        }
        Commands::Decrypt | Commands::Encrypt => {
            let mut process_file = |file: &PathBuf| {
                let file_data: Vec<u8> = read(file).unwrap();

                let (processed, new_extension) = match cli.command {
                    Commands::Decrypt => {
                        let decrypted: Vec<u8> = decrypter.decrypt(&file_data);
                        let extension: &str = file.extension().unwrap().to_str().unwrap();
                        let new_extension: &str = match extension {
                            "rpgmvp" | "png_" => "png",
                            "rpgmvo" | "ogg_" => "ogg",
                            "rpgmvm" | "m4a_" => "m4a",
                            _ => unreachable!(),
                        };
                        (decrypted, new_extension)
                    }

                    Commands::Encrypt => {
                        let encrypted: Vec<u8> = decrypter.encrypt(&file_data).unwrap();
                        let extension: &str = file.extension().unwrap().to_str().unwrap();
                        let new_extension: &str = match (engine.as_str(), extension) {
                            ("mv", "png") => "rpgmvp",
                            ("mv", "ogg") => "rpgmvo",
                            ("mv", "m4a") => "rpgmvm",
                            ("mz", "png") => "png_",
                            ("mz", "ogg") => "ogg_",
                            ("mz", "m4a") => "m4a_",
                            _ => unreachable!(),
                        };

                        (encrypted, new_extension)
                    }
                    _ => unreachable!(),
                };

                let output_file =
                    cli.output_dir.as_ref().unwrap_or(&cli.input_dir).join(
                        PathBuf::from(file.file_name().unwrap()).with_extension(new_extension),
                    );
                write(output_file, processed).unwrap();
            };

            if let Some(file) = &cli.file {
                process_file(file);
            } else {
                let extensions: &[&str] = match cli.command {
                    Commands::Encrypt => &["png", "ogg", "m4a"],
                    Commands::Decrypt => &["rpgmvp", "rpgmvo", "rpgmvm", "ogg_", "png_", "m4a_"],
                    _ => unreachable!(),
                };

                for entry in read_dir(&cli.input_dir).unwrap().flatten() {
                    let path: PathBuf = entry.path();

                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if extensions.contains(&ext) {
                            process_file(&path);
                        }
                    }
                }
            }
        }
    }

    println!("Elapsed: {:.2}s", start_time.elapsed().as_secs_f32());
}
