use std::fs::{self, File};
use std::io::{self, BufReader, Read, Write};
use std::path::Path;
use zip::ZipArchive;
use scraper::{Html, Selector};
use regex::Regex;
use htmlescape::decode_html;
use anyhow::Result;
use xml::reader::{EventReader, XmlEvent};

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
    let document = Html::parse_document(html);
    let selector = Selector::parse("body").map_err(|e| anyhow::anyhow!("Selector 解析失败: {}", e))?;

    let text = document.select(&selector)
        .flat_map(|element| element.text())
        .collect::<String>();

    let decoded = decode_html(&text).unwrap_or(text);

    // 清理零宽度空格等
    // let re = Regex::new(r"[\u200B\ufeff]").map_err(|e| anyhow::anyhow!("Regex 编译失败: {}", e))?;
    // let cleaned = re.replace_all(&decoded, "");

    let re = Regex::new(r"\n\s*\n").map_err(|e| anyhow::anyhow!("Regex 编译失败: {}", e))?;
    // Ok(re.replace_all(&cleaned, "\n\n").to_string())
    Ok(re.replace_all(&decoded, "\n\n").to_string())
}

fn extract_title(html: &str) -> Result<String> {
    let document = Html::parse_document(html);

    let title_selector = Selector::parse("title").map_err(|e| anyhow::anyhow!("Selector 解析失败: {}", e))?;
    if let Some(title_element) = document.select(&title_selector).next() {
        let title = title_element.text().collect::<String>().trim().to_string();
        if !title.is_empty() {
            return Ok(title);
        }
    }

    let h1_selector = Selector::parse("h1").map_err(|e| anyhow::anyhow!("Selector 解析失败: {}", e))?;
    Ok(document.select(&h1_selector)
        .next()
        .map(|e| e.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| "".to_string()))
}

fn remove_original_title(text: &str, title: &str) -> Result<String> {
    let pattern = format!(r"^\s*{}\s*", regex::escape(title));
    let re = Regex::new(&pattern).map_err(|e| anyhow::anyhow!("Regex 编译失败: {}", e))?;
    Ok(re.replace(text, "").trim_start().to_string())
}