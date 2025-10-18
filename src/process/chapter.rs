use anyhow::{Result, anyhow};
use kuchiki::NodeRef;
use kuchiki::traits::*;
use regex::Regex;
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

use super::opf::Metadata;

pub struct Chapter {
    pub index: usize,
    pub title: String,
    pub content: String,
}

impl Chapter {
    pub fn new(index: usize, title: String, content: String) -> Self {
        Chapter {
            index,
            title,
            content,
        }
    }

    fn create(
        archive: &mut ZipArchive<BufReader<File>>,
        chapter_file: &str,
        index: usize,
    ) -> Result<Chapter> {
        let text = read_chapter_content(archive, chapter_file)?;
        let title = extract_title(&text)?;
        let content = extract_text(&text)?;
        let content = remove_original_title(&content, &title)?;

        Ok(Self::new(index, title, content))
    }

    pub fn create_chapters(
        archive: &mut ZipArchive<BufReader<File>>,
        chapter_files: &[String],
    ) -> Result<Vec<Chapter>> {
        let mut chapters = Vec::new();
        for (index, chapter_file) in chapter_files.iter().enumerate() {
            let chapter = Chapter::create(archive, chapter_file, index)?;
            chapters.push(chapter);
        }
        Ok(chapters)
    }

    fn write(&self, output_dir: &Path) -> Result<()> {
        let chapter_path = output_dir.join(format!("chapter_{}.txt", self.index + 1));
        let mut chapter_file = File::create(chapter_path)?;
        writeln!(chapter_file, "{}\n\n{}", self.title, self.content)?;
        Ok(())
    }
}

    pub fn write_chapters(chapters: &[Chapter], output_dir: &Path) -> Result<()> {
        for chapter in chapters {
            chapter.write(output_dir)?;
        }
        Ok(())
    }

    pub fn write_total(metadata: Metadata, chapters: &[Chapter], output_dir: &Path, separator: &str) -> Result<()> {
        let output_path = output_dir.join(format!("{}.txt", metadata.title));
        let mut total_file = File::create(output_path)?;
        writeln!(total_file, "{}\n\n", metadata.title)?;
        if let Some(author) = &metadata.author {
            writeln!(total_file, "作者: {}\n\n", author)?;
        }
        if let Some(description) = &metadata.description {
            writeln!(total_file, "简介: {}\n\n", description)?;
        }
        for (index, chapter) in chapters.iter().enumerate() {
            writeln!(total_file, "{}\n\n{}", chapter.title, chapter.content)?;
            if !separator.is_empty() && index < chapters.len() - 1 {
                writeln!(total_file, "\n{}\n", separator)?;
            } else {
                writeln!(total_file, "\n")?;
            }
        }
        Ok(())
    }

fn read_chapter_content(
    archive: &mut ZipArchive<BufReader<File>>,
    chapter_file: &str,
) -> Result<String> {
    let mut file = archive.by_name(chapter_file)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;
    Ok(content)
}

/// 根据 OPF 文件位置解析章节路径
///
/// # 参数
/// - `hrefs`: 从 manifest 获取的原始 href 数组
/// - `opf_path`: OPF 文件在 ZIP 中的路径（如 "OEBPS/content.opf"）
///
/// # 返回
/// 相对于 EPUB 根目录的正确路径（如 "OEBPS/chapter1.xhtml"）
pub fn href2path(hrefs: &[String], opf_path: &str) -> Vec<String> {
    // 获取 OPF 文件所在目录（去掉文件名）
    let opf_dir = Path::new(opf_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));

    hrefs
        .iter()
        .map(|href| {
            // 1. 将 href 与 OPF 目录拼接
            let full_path = opf_dir.join(href);

            // 2. 规范化路径（处理 ./ 和 ../）
            let normalized = normalize_path(&full_path);

            // 3. 转换为字符串并统一使用 / 分隔符
            normalized.to_string_lossy().replace('\\', "/")
        })
        .collect()
}

/// 简单的路径规范化（不进行实际文件系统操作）
fn normalize_path(path: &Path) -> PathBuf {
    let mut stack = Vec::new();

    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if stack
                    .last()
                    .map_or(false, |c| c != &std::path::Component::ParentDir)
                {
                    stack.pop();
                } else {
                    stack.push(component);
                }
            }
            _ => stack.push(component),
        }
    }

    if stack.is_empty() {
        return PathBuf::from(".");
    }

    stack.iter().fold(PathBuf::new(), |mut acc, &c| {
        acc.push(c);
        acc
    })
}

fn extract_text(html: &str) -> Result<String> {
    let document = kuchiki::parse_html().one(html);
    let body = document
        .select("body")
        .unwrap()
        .next()
        .ok_or(anyhow!("未找到 body 元素"))?;
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
        nodes
            .next()
            .map(|node| node.text_contents().trim().to_string())
    })
}

fn remove_original_title(text: &str, title: &str) -> Result<String> {
    let pattern = format!(r"^\s*{}\s*", regex::escape(title));
    let re = Regex::new(&pattern).map_err(|e| anyhow::anyhow!("Regex 编译失败: {}", e))?;
    Ok(re.replace(text, "").trim_start().to_string())
}
