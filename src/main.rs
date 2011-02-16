use std::{fs::File, path::PathBuf};

use anyhow::Result;
use apk::Apk;
use clap::{
    builder::{
        styling::{AnsiColor, Style},
        Styles,
    },
    Parser,
};
use zip::ZipWriter;
use zip::{write::ExtendedFileOptions, ZipArchive};
#[derive(Parser)]
#[clap(name = "Mc injector", version = "0.0.1")]
#[command(version, about, long_about = None, styles = get_style())]
struct Options {
    /// Apk file to patch
    #[clap(required = true)]
    apk: PathBuf,
    /// Output path
    #[arg(short, long, required = true)]
    output: PathBuf,
}
const fn get_style() -> Styles {
    Styles::styled()
        .header(AnsiColor::BrightYellow.on_default())
        .usage(AnsiColor::Green.on_default())
        .literal(Style::new().fg_color(None).bold())
        .placeholder(AnsiColor::Green.on_default())
}
fn main() -> Result<()> {
    let options = Options::parse();
    let file = File::open(&options.apk)?;
    let output_file = File::create(&options.output)?;
    let mut input_apk = ZipArchive::new(file)?;
    let mut output_apk = ZipWriter::new(output_file);
    for i in 0..input_apk.len() {
        let mut file = input_apk.by_index(i)?;
        if is_v1sign(file.name()) {
            continue;
        }
        // Boo hoo alignment & compression
        if file.name() == "resources.arsc" {
            let options = zip::write::FileOptions::<ExtendedFileOptions>::default()
                .compression_method(zip::CompressionMethod::Stored)
                .with_alignment(4);
            output_apk.start_file(file.name(), options)?;
            std::io::copy(&mut file, &mut output_apk)?;
            continue;
        }
        output_apk.raw_copy_file(file)?;
    }
    output_apk.finish()?;
    println!("Signing apk with debug keystore");
    Apk::sign(&options.output, None)?;
    println!("Done");
    Ok(())
}

// Remove signature v1 if any
fn is_v1sign(filename: &str) -> bool {
    filename.starts_with("META-INF/") && (filename.ends_with(".SF") || filename.ends_with("RSA"))
}
