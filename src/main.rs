use anyhow::{anyhow, Result};
use kuchiki::traits::*;
use kuchiki::NodeRef;
use regex::Regex;
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use xml::attribute::OwnedAttribute;
use xml::reader::{EventReader, XmlEvent};
use zip::ZipArchive;

fn main() -> Result<()> {
    let epub_path = prompt_input("请输入 EPUB 文件路径:")?;
    let book_name = extract_book_name(&epub_path);
    let chapter_output_dir = create_output_dir(&book_name)?;

    let style_choice = prompt_number("1. 无编号\n2. 01 标题\n3. 第1章 标题\n请选择标题样式(默认为1):", 1)?;
    let (start_number, start_chapter) = get_start_number_and_chapter(style_choice)?;
    let number_digits = get_number_digits(style_choice)?;
    let separator = prompt_input("请输入分隔符（回车跳过）:")?;

    let mut archive = open_epub(&epub_path)?;
    let (manifest, spine) = parse_opf(&mut archive)?;
    let chapter_files = get_chapter_files(&spine, &manifest);

    let total_txt_path = Path::new("output").join(format!("{}.txt", book_name));
    let mut total_output_file = File::create(&total_txt_path)?;

    process_chapters(
        &mut archive,
        &chapter_files,
        &chapter_output_dir,
        &mut total_output_file,
        style_choice,
        start_number,
        start_chapter,
        number_digits,
        &separator,
    )?;

    println!("转换完成！");
    Ok(())
}

fn extract_book_name(epub_path: &str) -> &str {
    Path::new(epub_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output")
}

fn create_output_dir(book_name: &str) -> Result<PathBuf> {
    let chapter_output_dir = Path::new("output").join(book_name);
    fs::create_dir_all(&chapter_output_dir)?;
    Ok(chapter_output_dir)
}

fn get_start_number_and_chapter(style_choice: usize) -> Result<(usize, usize)> {
    if style_choice == 2 || style_choice == 3 {
        let start_number = prompt_number("请输入起始编号（≥0，默认为1）:", 1)?;
        let start_chapter = prompt_number("请输入从第几章开始添加编号（默认为1）:", 1)?;
        Ok((start_number, start_chapter))
    } else {
        Ok((0, 1))
    }
}

fn get_number_digits(style_choice: usize) -> Result<usize> {
    if style_choice == 2 {
        prompt_number("请输入编号位数(默认为2):", 2)
    } else {
        Ok(0)
    }
}

fn open_epub(epub_path: &str) -> Result<ZipArchive<BufReader<File>>> {
    let file = File::open(epub_path)?;
    Ok(ZipArchive::new(BufReader::new(file))?)
}

fn parse_opf(archive: &mut ZipArchive<BufReader<File>>) -> Result<(Vec<(String, String)>, Vec<String>)> {
    let opf_file = archive.by_name("OEBPS/content.opf")?;
    let parser = EventReader::new(opf_file);
    let mut manifest = Vec::new();
    let mut spine = Vec::new();
    let mut in_manifest = false;
    let mut in_spine = false;

    for event in parser {
        match event? {
            XmlEvent::StartElement { name, attributes, .. } => {
                if name.local_name == "manifest" {
                    in_manifest = true;
                } else if name.local_name == "item" && in_manifest {
                    if let (Some(id), Some(href)) = get_id_and_href(&attributes) {
                        manifest.push((id, href));
                    }
                } else if name.local_name == "spine" {
                    in_spine = true;
                } else if name.local_name == "itemref" && in_spine {
                    if let Some(idref) = get_idref(&attributes) {
                        if !idref.contains("nav") && !idref.contains("cover") {
                            spine.push(idref);
                        }
                    }
                }
            }
            XmlEvent::EndElement { name } => {
                if name.local_name == "manifest" {
                    in_manifest = false;
                } else if name.local_name == "spine" {
                    in_spine = false;
                }
            }
            _ => {}
        }
    }

    Ok((manifest, spine))
}

fn get_id_and_href(attributes: &[OwnedAttribute]) -> (Option<String>, Option<String>) {
    let id = attributes.iter().find(|attr| attr.name.local_name == "id").map(|attr| attr.value.clone());
    let href = attributes.iter().find(|attr| attr.name.local_name == "href").map(|attr| attr.value.clone());
    (id, href)
}

fn get_idref(attributes: &[OwnedAttribute]) -> Option<String> {
    attributes.iter().find(|attr| attr.name.local_name == "idref").map(|attr| attr.value.clone())
}
fn get_chapter_files(spine: &[String], manifest: &[(String, String)]) -> Vec<String> {
    spine.iter()
        .filter_map(|idref| {
            manifest.iter()
                .find(|(id, _)| id == idref)
                .map(|(_, href)| format!("OEBPS/{}", href))
        })
        .collect()
}

fn process_chapters(
    archive: &mut ZipArchive<BufReader<File>>,
    chapter_files: &[String],
    chapter_output_dir: &Path,
    total_output_file: &mut File,
    style_choice: usize,
    start_number: usize,
    start_chapter: usize,
    number_digits: usize,
    separator: &str,
) -> Result<()> {
    let mut chapter_number = start_number;
    for (index, chapter_file) in chapter_files.iter().enumerate() {
        let content = read_chapter_content(archive, chapter_file)?;
        let title = extract_title(&content)?;
        let text = extract_text(&content)?;
        let text = remove_original_title(&text, &title)?;

        let numbered_title = get_numbered_title(
            index + 1,
            &title,
            style_choice,
            chapter_number,
            number_digits,
            start_chapter,
        );

        write_chapter_file(
            chapter_output_dir,
            index + 1,
            &numbered_title,
            &text,
        )?;

        write_total_output_file(
            total_output_file,
            &numbered_title,
            &text,
            separator,
            index,
            chapter_files.len(),
        )?;

        if index + 1 >= start_chapter {
            chapter_number += 1;
        }
    }
    Ok(())
}

fn read_chapter_content(archive: &mut ZipArchive<BufReader<File>>, chapter_file: &str) -> Result<String> {
    let mut file = archive.by_name(chapter_file)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;
    Ok(content)
}

fn get_numbered_title(
    index: usize,
    title: &str,
    style_choice: usize,
    chapter_number: usize,
    number_digits: usize,
    start_chapter: usize,
) -> String {
    if index >= start_chapter {
        match style_choice {
            3 => format!("第{}章 {}", chapter_number, title),
            2 => format!("{:0width$} {}", chapter_number, title, width = number_digits),
            _ => title.to_string(),
        }
    } else {
        title.to_string()
    }
}

fn write_chapter_file(
    chapter_output_dir: &Path,
    index: usize,
    numbered_title: &str,
    text: &str,
) -> Result<()> {
    let chapter_path = chapter_output_dir.join(format!("chapter_{:03}.txt", index));
    let mut chapter_file = File::create(chapter_path)?;
    writeln!(chapter_file, "{}\n\n{}", numbered_title, text)?;
    Ok(())
}

fn write_total_output_file(
    total_output_file: &mut File,
    numbered_title: &str,
    text: &str,
    separator: &str,
    index: usize,
    total_chapters: usize,
) -> Result<()> {
    writeln!(total_output_file, "{}\n\n{}", numbered_title, text)?;
    if !separator.is_empty() && index < total_chapters - 1 {
        writeln!(total_output_file, "\n{}\n", separator)?;
    } else {
        writeln!(total_output_file, "\n")?;
    }
    Ok(())
}


fn prompt_input(prompt: &str) -> Result<String> {
    print!("{}", prompt);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim()
        .trim_matches('"')
        .trim()
        .to_string())
}

fn prompt_number(prompt: &str, default: usize) -> Result<usize> {
    loop {
        print!("{}", prompt);
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim()
            .trim_matches('"')
            .trim();
        if input.is_empty() {
            return Ok(default);
        }
        match input.parse::<usize>() {
            Ok(n) => return Ok(n),
            Err(_) => println!("请输入有效的非负整数"),
        }
    }
}

fn extract_text(html: &str) -> Result<String> {
    let document = kuchiki::parse_html().one(html);
    let body = document.select("body").unwrap().next().ok_or(anyhow!("未找到 body 元素"))?;
    let mut text = String::new();
    extract_text_from_node(&body.as_node(), &mut text);
    let unescaped = html_escape::decode_html_entities(&text);
    Ok(unescaped.parse()?)
}

fn extract_text_from_node(node: &NodeRef, text: &mut String) {
    let text_tags = ["p", "h1", "h2", "h3", "h4", "h5", "h6"];

    for child in node.inclusive_descendants() {
        if let Some(element) = child.as_element() {
            if text_tags.contains(&element.name.local.to_string().as_str()) {
                text.push('\n');
            }
        } else if let Some(text_node) = child.as_text() {
            let borrowed_text = text_node.borrow();
            let content = if is_parent_text_tag(&child, &text_tags) {
                borrowed_text.to_string()
            } else {
                borrowed_text.trim().to_string()
            };

            if !content.is_empty() {
                text.push_str(&content);
            }
        }
    }

    text.trim().to_string();
}

fn is_parent_text_tag(node: &NodeRef, text_tags: &[&str]) -> bool {
    if let Some(parent) = node.parent() {
        if let Some(element) = parent.as_element() {
            return text_tags.contains(&element.name.local.to_string().as_str());
        }
    }
    false
}

fn extract_title(html: &str) -> Result<String> {
    let document = kuchiki::parse_html().one(html);

    let title_tags = ["title", "h1", "h2", "h3", "h4", "h5", "h6"];

    for tag in &title_tags {
        if let Some(title) = extract_text_from_selector(&document, tag) {
            let trimmed_title = title.trim();
            if !trimmed_title.is_empty() {
                return Ok(trimmed_title.to_string());
            }
        }
    }

    Ok("".to_string())
}

fn extract_text_from_selector(document: &NodeRef, selector: &str) -> Option<String> {
    document.select(selector).ok().and_then(|mut nodes| {
        nodes.next().map(|node| {
            node.text_contents().trim().to_string()
        })
    })
}

fn remove_original_title(text: &str, title: &str) -> Result<String> {
    let pattern = format!(r"^\s*{}\s*", regex::escape(title));
    let re = Regex::new(&pattern).map_err(|e| anyhow::anyhow!("Regex 编译失败: {}", e))?;
    Ok(re.replace(text, "").trim_start().to_string())
}

fn _extract_text_from_node(node: &NodeRef, text: &mut String) {
    for child in node.inclusive_descendants() {
        if let Some(element) = child.as_element() {
            if element.name.local.to_string() == "br" {
                text.push('\n');
            }
        } else if let Some(text_node) = child.as_text() {
            let borrowed_text = text_node.borrow();
            let content = borrowed_text.trim();
            if !content.is_empty() {
                text.push_str(content);
                text.push('\n');
            }
        }
    }
}
