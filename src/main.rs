use asset_decrypter::Decrypter;
use clap::{Parser, Subcommand, crate_version, value_parser};
use std::{
    fs::{read, read_dir, read_to_string, write},
    path::PathBuf,
};

#[derive(Parser)]
#[command(version = crate_version!(), about = "Decrypt/encrypt RPG Maker MV/MZ audio and image assets.")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Encryption key for `encrypt` command or `decrypt` when decrypting audio files. Can be also found in System.json file
    #[arg(short = 'e', long, global = true)]
    encryption_key: Option<String>,

    /// Game engine.
    #[arg(short = 'E', long, value_parser = ["mv", "mz"], global = true)]
    engine: Option<String>,

    /// Path to the input directory.
    #[arg(short, long, default_value = "./", value_parser = value_parser!(PathBuf), hide_default_value = true, global = true)]
    input_dir: PathBuf,

    /// Path to the output directory.
    #[arg(short, long, default_value = "./", value_parser = value_parser!(PathBuf), hide_default_value = true, global = true)]
    output_dir: PathBuf,

    /// Encrypted image file path or System.json path to extract key from when using extract-key command.
    #[arg(short, long, value_parser = value_parser!(PathBuf), global = true)]
    file: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Encrypt the files.
    Encrypt,
    /// Decrypt the files.
    Decrypt,
    /// Extract the key from encrypted image file. Requires --file argument.
    ExtractKey,
}

fn main() {
    let cli: Cli = Cli::parse();

    if cli.engine.is_none() {
        eprintln!("--engine argument is not specified.");
        return;
    };

    let engine: String = cli.engine.unwrap();

    let mut decrypter: Decrypter = Decrypter::new(cli.encryption_key);

    if let Commands::ExtractKey = cli.command {
        if let Some(file) = cli.file {
            let key: String = if file.extension().unwrap() == "json" {
                let content: String = read_to_string(file).unwrap();
                let encryption_key_index: usize = content.rfind("encryptionKey").unwrap();
                let after_encryption_key: &str =
                    &content[encryption_key_index + "encryptionKey\":".len()..];
                let starting_quote_index: usize = after_encryption_key.find('"').unwrap();
                after_encryption_key[starting_quote_index + 1
                    ..starting_quote_index + 1 + "d41d8cd98f00b204e9800998ecf8427e".len()]
                    .to_owned()
            } else {
                let buf: Vec<u8> = read(file).unwrap();

                decrypter.set_key_from_image(&buf);
                decrypter.key()
            };

            println!("Encryption key: {key}");
            return;
        } else {
            eprintln!("--file argument is not specified.");
            return;
        }
    }

    let file_extensions: &[&str] = match cli.command {
        Commands::Encrypt => &["png", "ogg", "m4a"],
        Commands::Decrypt => &["rpgmvp", "rpgmvo", "rpgmvm", "ogg_", "png_", "m4a_"],
        Commands::ExtractKey => &[],
    };

    let entries = read_dir(&cli.input_dir)
        .unwrap()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            path.extension()
                .and_then(|ext| ext.to_owned().into_string().ok())
                .filter(|ext| file_extensions.contains(&ext.as_str()))
                .map(|ext| (path, ext.to_string()))
        });

    for (path, ext) in entries {
        let data = read(&path).unwrap();

        let (processed_data, new_ext) = match cli.command {
            Commands::Encrypt => {
                let encrypted: Vec<u8> = decrypter.encrypt(&data);
                let new_ext: &str = match (engine.as_str(), ext.as_str()) {
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
            Commands::Decrypt => {
                let decrypted: Vec<u8> = decrypter.decrypt(&data);
                let new_ext: &str = match ext.as_str() {
                    "rpgmvp" | "png_" => "png",
                    "rpgmvo" | "ogg_" => "ogg",
                    "rpgmvm" | "m4a_" => "m4a",
                    _ => unreachable!(),
                };
                (decrypted, new_ext)
            }
            _ => unreachable!(),
        };

        let output_file: PathBuf = cli
            .output_dir
            .join(PathBuf::from(path.file_name().unwrap()).with_extension(new_ext));

        write(output_file, processed_data).unwrap();
    }
}
