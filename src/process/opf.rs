use std::collections::HashMap;
use std::fs::{self, File};
use std::io::BufReader;
use std::io::Read;
use std::path::Path;

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, value};
use xml::attribute::OwnedAttribute;
use xml::reader::{EventReader, XmlEvent};
use zip::ZipArchive;

#[derive(Debug, Serialize, Deserialize)]
pub struct Metadata {
    pub title: String,
    pub author: Option<String>,
    pub language: Option<String>,
    pub description: Option<String>,
    pub subject: Vec<String>,
}

impl Default for Metadata {
    fn default() -> Self {
        Metadata {
            title: String::new(),
            author: None,
            language: None,
            description: None,
            subject: Vec::new(),
        }
    }
}

impl Metadata {
    pub fn write(&self, output_dir: &Path) -> Result<()> {
        let path = output_dir.join("metadata.toml");
        // 创建 TOML 文档
        let mut doc = DocumentMut::new();

        // 添加 title (必填字段)
        doc["title"] = value(self.title.clone());

        // 添加可选字段
        if let Some(author) = &self.author {
            doc["author"] = value(author.clone());
        }

        if let Some(language) = &self.language {
            doc["language"] = value(language.clone());
        }

        if let Some(description) = &self.description {
            doc["description"] = value(description.clone());
        }

        // 处理数组字段 subject
        if !self.subject.is_empty() {
            let mut array = toml_edit::Array::new();
            for subject in &self.subject {
                array.push(subject);
            }
            doc["subject"] = Item::Value(toml_edit::Value::Array(array));
        }

        // 写入文件
        fs::write(path, doc.to_string())?;
        Ok(())
    }
}

// 解析 OPF 文件，返回按 spine 顺序排列的章节文件路径
pub fn parse_opf(
    archive: &mut ZipArchive<BufReader<File>>,
    opf_path: &str,
) -> Result<(Metadata, Vec<String>)> {
    let opf_content = get_opf_content(archive, opf_path)?;

    let metadata = parse_metadata(&opf_content)?;

    // 1. 解析 manifest 建立 id -> href 的映射
    let manifest = parse_manifest(&opf_content)?;
    let id_to_href: HashMap<String, String> = manifest.into_iter().collect();

    // 2. 解析 spine 获取章节顺序
    let spine_ids = parse_spine(&opf_content)?;

    // 3. 将 spine id 转换为对应的 href 路径
    let mut chapter_paths = Vec::new();
    for id in spine_ids {
        if let Some(href) = id_to_href.get(&id) {
            chapter_paths.push(href.clone());
        } else {
            return Err(anyhow!("Missing href for chapter id: {}", id));
        }
    }

    Ok((metadata, chapter_paths))
}

/// 解析 OPF 文件中的元数据部分
fn parse_metadata(opf_content: &str) -> Result<Metadata> {
    let parser = EventReader::new(opf_content.as_bytes());
    let mut metadata = Metadata::default();
    let mut in_metadata = false;
    let mut current_tag = String::new();
    let mut current_value = String::new();
    let mut subjects = Vec::new();

    for event in parser {
        match event? {
            XmlEvent::StartElement { name, .. } => {
                if name.local_name == "metadata" {
                    in_metadata = true;
                } else if in_metadata {
                    current_tag = name.local_name.clone();
                    match current_tag.as_str() {
                        "title" | "creator" | "language" | "description" | "subject" => {
                            current_value.clear();
                        }
                        _ => {}
                    }
                }
            }
            XmlEvent::Characters(text) => {
                if in_metadata {
                    match current_tag.as_str() {
                        "title" | "creator" | "language" | "description" | "subject" => {
                            current_value.push_str(&text);
                        }
                        _ => {}
                    }
                }
            }
            XmlEvent::EndElement { name } => {
                if name.local_name == "metadata" {
                    in_metadata = false;
                } else if in_metadata {
                    match name.local_name.as_str() {
                        "title" => {
                            metadata.title = current_value.trim().to_string();
                            current_value.clear();
                        }
                        "creator" => {
                            metadata.author = Some(current_value.trim().to_string());
                            current_value.clear();
                        }
                        "language" => {
                            metadata.language = Some(current_value.trim().to_string());
                            current_value.clear();
                        }
                        "description" => {
                            metadata.description = Some(current_value.trim().to_string());
                            current_value.clear();
                        }
                        "subject" => {
                            subjects.push(current_value.trim().to_string());
                            current_value.clear();
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    metadata.subject = subjects;
    Ok(metadata)
}

/// 解析 OPF 内容中的 manifest 部分
fn parse_manifest(opf_content: &str) -> Result<Vec<(String, String)>> {
    let parser = EventReader::new(opf_content.as_bytes());
    let mut manifest = Vec::new();
    let mut in_manifest = false;

    for event in parser {
        match event? {
            XmlEvent::StartElement {
                name, attributes, ..
            } => {
                if name.local_name == "manifest" {
                    in_manifest = true;
                } else if name.local_name == "item" && in_manifest {
                    if let (Some(id), Some(href)) = get_id_and_href(&attributes) {
                        manifest.push((id, href));
                    }
                }
            }
            XmlEvent::EndElement { name } => {
                if name.local_name == "manifest" {
                    in_manifest = false;
                }
            }
            _ => {}
        }
    }

    Ok(manifest)
}

/// 解析 OPF 内容中的 spine 部分
fn parse_spine(opf_content: &str) -> Result<Vec<String>> {
    let parser = EventReader::new(opf_content.as_bytes());
    let mut spine = Vec::new();
    let mut in_spine = false;

    for event in parser {
        match event? {
            XmlEvent::StartElement {
                name, attributes, ..
            } => {
                if name.local_name == "spine" {
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
                if name.local_name == "spine" {
                    in_spine = false;
                }
            }
            _ => {}
        }
    }

    Ok(spine)
}

fn get_id_and_href(attributes: &[OwnedAttribute]) -> (Option<String>, Option<String>) {
    let id = attributes
        .iter()
        .find(|attr| attr.name.local_name == "id")
        .map(|attr| attr.value.clone());
    let href = attributes
        .iter()
        .find(|attr| attr.name.local_name == "href")
        .map(|attr| attr.value.clone());
    (id, href)
}

fn get_idref(attributes: &[OwnedAttribute]) -> Option<String> {
    attributes
        .iter()
        .find(|attr| attr.name.local_name == "idref")
        .map(|attr| attr.value.clone())
}

fn get_opf_content(archive: &mut ZipArchive<BufReader<File>>, opf_path: &str) -> Result<String> {
    let mut opf_file = archive.by_name(opf_path)?;
    let mut content = String::new();
    opf_file.read_to_string(&mut content)?;
    Ok(content)
}

pub fn get_opf_path(
    archive: &mut zip::ZipArchive<std::io::BufReader<std::fs::File>>,
) -> Result<String> {
    let container_file = archive.by_name("META-INF/container.xml")?;

    let parser = EventReader::new(container_file);
    let mut opf_path = None;
    let mut in_rootfiles = false;

    for event in parser {
        match event {
            Ok(XmlEvent::StartElement {
                name, attributes, ..
            }) => match name.local_name.as_str() {
                "rootfiles" => in_rootfiles = true,
                "rootfile" if in_rootfiles => {
                    opf_path = attributes
                        .iter()
                        .find(|attr| attr.name.local_name == "full-path")
                        .map(|attr| attr.value.clone());
                }
                _ => {}
            },
            Ok(XmlEvent::EndElement { name }) => {
                if name.local_name == "rootfiles" {
                    in_rootfiles = false;
                }
            }
            Err(e) => return Err(anyhow!("XML parsing error: {}", e)),
            _ => {}
        }
    }

    // 3. 确保获取到有效路径
    opf_path.ok_or_else(|| anyhow!("No OPF path found in container.xml"))
}
