mod chapter;
mod metadata;

use std::fs::File;
use std::io::Write;
use std::io::BufReader;
use std::path::PathBuf;

use anyhow::Result;
use zip::ZipArchive;
use quick_xml::Reader;
use quick_xml::events::Event;

use chapter::ChapterIter;
use metadata::{Metadata, Package};
use crate::config::get_config;
use crate::utils::normalize_zip_path;

pub struct Epub {
    pub filename: String,
    pub archive: ZipArchive<File>,
    pub metadata: Metadata,
    pub chapters: Vec<String>,
}

impl Epub {
    pub fn from_file(epub_path: PathBuf) -> Result<Self> {
        let filename = epub_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid EPUB file name"))?;
        let filename = filename.to_string();

        let file = File::open(&epub_path)?;

        let mut epub = ZipArchive::new(file)?;

        let opf_path = Self::extract_opf_path(&mut epub)?;
        let package = {
            let mut opf_file = epub.by_name(&opf_path)?;
            Package::from_opf(&mut opf_file)?
        };

        let idhref_map = package.manifest.into_map();
        let spine_hrefs = package.spine.into_hrefs(idhref_map);
        let metadata = package.metadata;

        let chapters = spine_hrefs
            .into_iter()
            .map(|href| normalize_zip_path(&opf_path, href))
            .collect::<Vec<String>>();

        Ok(Self {
            metadata,
            archive: epub,
            filename,
            chapters,
        })
    }

    pub fn output_dir(&self) -> Result<PathBuf> {
        let output_dir = PathBuf::from(&get_config().output_dir).join(&self.filename);
        if !output_dir.exists() {
            std::fs::create_dir_all(&output_dir)?;
        }
        Ok(output_dir)
    }

    pub fn chapters_output(&self) -> Result<PathBuf> {
        let chapters_dir = self.output_dir()?.join("chapters");
        if !chapters_dir.exists() {
            std::fs::create_dir(&chapters_dir)?;
        }
        Ok(chapters_dir)
    }

    pub fn total_path(&self) -> Result<PathBuf> {
        let output_dir = self.output_dir()?;
        let total_path = output_dir.join(format!(
            "{}.txt",
            &self
                .metadata
                .title
                .as_deref()
                .unwrap_or(self.filename.as_str())
        ));
        if !total_path.exists() {
            File::create(&total_path)?;
        }
        Ok(total_path)
    }

    pub fn write_metadata(&self) -> Result<()> {
        let output_dir = self.output_dir()?;
        self.metadata.write(&output_dir)
    }

    pub fn write(&mut self) -> Result<()> {
        if get_config().options.metadata {
            self.write_metadata()?;
        }

        let chapters_dir = if get_config().options.split {
            Some(self.chapters_output()?)
        } else {
            None
        };
        let total_path = if get_config().options.combine {
            Some(self.total_path()?)
        } else {
            None
        };

        if chapters_dir.is_none() && total_path.is_none() {
            return Ok(());
        }

        let chapters = self.get_chapters()?;

        for (index, chapter) in chapters.enumerate() {
            let chapter = chapter?;
            if let Some(dir) = &chapters_dir {
                chapter.write(dir, index + 1)?;
            }

            if let Some(total_path) = &total_path {
                let mut file = File::options().append(true).open(total_path)?;
                writeln!(file, "{}\n", chapter.title)?;
                writeln!(file, "{}", chapter.content)?;
                writeln!(file, "\n{}\n", &get_config().separator)?;
            }
        }

        Ok(())
    }

    pub fn get_chapters(&mut self) -> Result<ChapterIter<'_>> {
        Ok(ChapterIter::new(&mut self.archive, &self.chapters))
    }

    fn extract_opf_path(epub: &mut ZipArchive<File>) -> Result<String> {
        let container: zip::read::ZipFile<'_, File> = epub.by_name("META-INF/container.xml")?;
        let container = BufReader::new(container);
        let mut reader = Reader::from_reader(container);
        reader.config_mut().trim_text(true);
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.name().as_ref() == b"rootfile" => {
                    for attr in e.attributes() {
                        let attr = attr?;
                        if attr.key.as_ref() == b"full-path" {
                            return Ok(String::from_utf8(attr.value.into_owned())?);
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(anyhow::anyhow!("Error reading XML: {}", e)),
                _ => {}
            }
            buf.clear();
        }
        Err(anyhow::anyhow!("OPF path not found in container.xml"))
    }
}
