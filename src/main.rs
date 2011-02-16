use std::{
    fs::File,
    io::{BufReader, Cursor, Read, Seek, Write},
    path::PathBuf,
};

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
mod manifest;
use zip::{write::ExtendedFileOptions, ZipArchive};
#[derive(Parser)]
#[clap(name = "Apksigner", version = "0.0.1")]
#[command(version, about, long_about = None, styles = get_style())]
struct Options {
    /// Apk file to sign/edit
    #[clap(required = true)]
    apk: PathBuf,
    // Set package name of apk
    #[arg(short, long, value_parser = validate_pkgname)]
    pkgname: Option<String>,
    // Set displayed name of apk
    #[arg(short, long)]
    appname: Option<String>,
    /// Output path
    #[arg(short, long, required = true)]
    output: PathBuf,
}
fn validate_pkgname(pkgname: &str) -> Result<String, String> {
    if !pkgname.is_ascii() {
        return Err("Input packagename is not ascii".to_string());
    }
    for char in pkgname.chars() {
        if !char.is_alphabetic() && char != '.' {
            return Err(
                "Package name can only contain alphabetical characters or '' separator".to_string(),
            );
        }
    }
    Ok(pkgname.to_ascii_lowercase())
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
    let file = BufReader::new(File::open(&options.apk)?);
    let output_file = File::create(&options.output)?;
    let mut input_apk = ZipArchive::new(file)?;
    let mut output_apk = ZipWriter::new(output_file);
    let has_oldsign = input_apk.file_names().any(is_v1sign);
    if !has_oldsign {
        println!("Fast editing!");
        fast_edit(input_apk, output_apk, &options)?;
        println!("Signing apk with debug keystore");
        Apk::sign(&options.output, None)?;
        println!("Done");
        return Ok(());
    }
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
        if file.name() == "AndroidManifest.xml"
            && (options.pkgname.is_some() || options.appname.is_some())
        {
            let mut file_data = Vec::with_capacity(file.size().try_into()?);
            file.read_to_end(&mut file_data)?;
            let edited = manifest::edit_manifest(
                &file_data,
                options.appname.as_deref(),
                options.pkgname.as_deref(),
            )?;
            output_apk.start_file(
                file.name(),
                zip::write::FileOptions::<ExtendedFileOptions>::default(),
            )?;
            output_apk.write_all(&edited)?;
            println!("Edited manifest");
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
fn fast_edit<R, W>(
    mut input: ZipArchive<R>,
    mut writer: ZipWriter<W>,
    opts: &Options,
) -> anyhow::Result<()>
where
    R: Read + Seek,
    W: Write + Seek,
{
    let temp_zip = Cursor::new(Vec::new());
    let mut patch_zip = ZipWriter::new(temp_zip);
    if let Ok(mut file) = input.by_name("AndroidManifest.xml") {
        let mut file_data = Vec::with_capacity(file.size().try_into()?);
        file.read_to_end(&mut file_data)?;
        let edited =
            manifest::edit_manifest(&file_data, opts.appname.as_deref(), opts.pkgname.as_deref())?;
        patch_zip.start_file(
            file.name(),
            zip::write::FileOptions::<ExtendedFileOptions>::default(),
        )?;
        patch_zip.write_all(&edited)?;
        println!("Edited manifest");
    }
    if let Ok(mut file) = input.by_name("resources.arsc") {
        let options = zip::write::FileOptions::<ExtendedFileOptions>::default()
            .compression_method(zip::CompressionMethod::Stored)
            .with_alignment(4);
        patch_zip.start_file(file.name(), options)?;
        std::io::copy(&mut file, &mut patch_zip)?;
    }
    writer.merge_archive(input)?;
    writer.merge_archive(patch_zip.finish_into_readable()?)?;
    writer.finish()?;
    Ok(())
}
// Remove signature v1 if any
fn is_v1sign(filename: &str) -> bool {
    filename.starts_with("META-INF/") && (filename.ends_with(".SF") || filename.ends_with("RSA"))
}
