use std::path::Path;
use std::fs;

use anyhow::Result;

use epub2txt::process::chapter::{write_total, write_chapters, Chapter};
use epub2txt::process::opf::{get_opf_path, parse_opf};
use epub2txt::process::{chapter, open_epub};

static OUTPUT: &str = "output";
static INPUT: &str = "input";

fn main() -> Result<()> {
    let base_dir = Path::new(OUTPUT);
    
    if !base_dir.exists() {
        fs::create_dir(base_dir)?;
    }
    
    let input_dir = Path::new(INPUT);

    if !input_dir.exists() {
        return Err(anyhow::anyhow!("Input directory does not exist"));
    }

    for entry in fs::read_dir(input_dir)? {
        let entry = entry?;
        let path = entry.path();

        // 检查是否是 EPUB 文件
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "epub" {
                    epub2txt(&base_dir, &path)?;
                }
            }
        }
    }

    Ok(())
}

fn epub2txt(base_dir: &Path, epub_path: &Path) -> Result<()> {
    let book_name = extract_book_name(&epub_path);
    let output_dir = base_dir.join(&book_name);
    let chapters_dir = output_dir.join("chapters");
    fs::create_dir(&output_dir)?;
    fs::create_dir(&chapters_dir)?;

    let separator = prompt_input("请输入分隔符（回车跳过）:")?;

    let mut archive = open_epub(epub_path)?;
    let opf_path = get_opf_path(&mut archive)?;
    let (metadata, chapter_hrefs) = parse_opf(&mut archive, &opf_path)?;
    let chapter_files = chapter::href2path(&chapter_hrefs, &opf_path);
    let chapters = Chapter::create_chapters(&mut archive, &chapter_files)?;
    metadata.write(&output_dir)?;
    write_chapters(&chapters, &chapters_dir)?;
    write_total(metadata, &chapters, &output_dir, &separator)?;

    Ok(())
}

fn prompt_input(prompt: &str) -> Result<String> {
    use std::io::{self, Write};

    print!("{}", prompt);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn extract_book_name(epub_path: &Path) -> String {
    epub_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown_book")
        .to_string()
}
