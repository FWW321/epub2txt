use anyhow::{anyhow, Result};
use kuchiki::traits::*;
use kuchiki::NodeRef;
use regex::Regex;
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Write};
use std::path::Path;
use xml::reader::{EventReader, XmlEvent};
use zip::ZipArchive;

fn main() -> Result<()> {
    let epub_path = prompt_input("请输入 EPUB 文件路径:")?;

    let book_name = Path::new(&epub_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    let chapter_output_dir = Path::new("output").join(book_name);
    fs::create_dir_all(&chapter_output_dir)?;

    let style_choice = prompt_input("1. 第x章 标题\n2. 01 标题\n3. 无编号\n请选择标题样式:")?;

    let (start_number, start_chapter) = if style_choice == "1" || style_choice == "2" {
        let start_number = prompt_number("请输入起始编号（≥0，默认为1）:", 1)?;
        let start_chapter = prompt_number("请输入从第几章开始添加编号（默认为1）:", 1)?;
        (start_number, start_chapter)
    } else {
        (0, 1)
    };

    let number_digits = if style_choice == "2" {
        prompt_number("请输入编号位数:", 2)?
    } else {
        0
    };

    let separator = prompt_input("请输入分隔符（回车跳过）:")?;

    let file = File::open(&epub_path)?;
    let mut archive = ZipArchive::new(BufReader::new(file))?;

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
                    let id = attributes.iter().find(|attr| attr.name.local_name == "id").map(|attr| attr.value.clone());
                    let href = attributes.iter().find(|attr| attr.name.local_name == "href").map(|attr| attr.value.clone());
                    if let (Some(id), Some(href)) = (id, href) {
                        manifest.push((id, href));
                    }
                } else if name.local_name == "spine" {
                    in_spine = true;
                } else if name.local_name == "itemref" && in_spine {
                    let idref = attributes.iter().find(|attr| attr.name.local_name == "idref").map(|attr| attr.value.clone());
                    if let Some(idref) = idref {
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

    let chapter_files = spine.into_iter()
        .filter_map(|idref| {
            manifest.iter()
                .find(|(id, _)| id == &idref)
                .map(|(_, href)| format!("OEBPS/{}", href))
        })
        .collect::<Vec<_>>();

    let total_txt_path = Path::new("output").join(format!("{}.txt", book_name));
    let mut total_output_file = File::create(&total_txt_path)?;

    let mut chapter_number = start_number;
    for (index, chapter_file) in chapter_files.iter().enumerate() {
        let mut file = archive.by_name(chapter_file)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;

        let title = extract_title(&content)?;
        let text = extract_text(&content)?;
        let text = remove_original_title(&text, &title)?;

        let numbered_title = if index + 1 >= start_chapter {
            match style_choice.as_str() {
                "1" => format!("第{}章 {}", chapter_number, title),
                "2" => format!("{:0width$} {}", chapter_number, title, width = number_digits),
                _ => title.to_string(),
            }
        } else {
            title.to_string()
        };

        let chapter_path = chapter_output_dir.join(format!("chapter_{:03}.txt", index + 1));
        let mut chapter_file = File::create(chapter_path)?;
        writeln!(chapter_file, "{}\n\n{}", numbered_title, text)?;

        writeln!(total_output_file, "{}\n\n{}", numbered_title, text)?;

        if !separator.is_empty() && index < chapter_files.len() - 1 {
            writeln!(total_output_file, "\n{}\n", separator)?;
        }

        if index + 1 >= start_chapter {
            chapter_number += 1;
        }
    }

    println!("转换完成！");
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
    for child in node.inclusive_descendants() {
        if let Some(element) = child.as_element() {
            if element.name.local.to_string() == "p" {
                text.push('\n');
            }
        } else if let Some(text_node) = child.as_text() {
            let borrowed_text = text_node.borrow();
            let content = if is_parent_p(&child) {
                borrowed_text.to_string()
            } else {
                borrowed_text.trim().to_string()
            };

            if !content.is_empty() {
                text.push_str(&content);
            }
        }
    }

    text.push_str("\n\n");
}

fn is_parent_p(node: &NodeRef) -> bool {
    if let Some(parent) = node.parent() {
        if let Some(element) = parent.as_element() {
            return element.name.local.to_string() == "p";
        }
    }
    false
}

fn extract_title(html: &str) -> Result<String> {
    let document = kuchiki::parse_html().one(html);

    if let Some(title) = extract_text_from_selector(&document, "title") {
        if !title.trim().is_empty() {
            return Ok(title.trim().to_string());
        }
    }

    if let Some(h1) = extract_text_from_selector(&document, "h1") {
        return Ok(h1.trim().to_string());
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