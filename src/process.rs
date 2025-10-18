pub mod chapter;
pub mod opf;

use std::fs::File;
use std::path::Path;
use std::io::BufReader;

use anyhow::Result;
use zip::ZipArchive;

pub fn open_epub(epub_path: &Path) -> Result<ZipArchive<BufReader<File>>> {
    let file = File::open(epub_path)?;
    Ok(ZipArchive::new(BufReader::new(file))?)
}
