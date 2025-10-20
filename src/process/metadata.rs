use std::path::Path;
use std::io::BufReader;
use std::fs::{self, File};

use quick_xml::de;
use anyhow::Result;
use ahash::AHashMap;
use zip::read::ZipFile;
use serde::Deserialize;
use phf::{Map, phf_map};
use toml_edit::{DocumentMut, Item, value};

pub static ROLE_MAP: Map<&'static str, &'static str> = phf_map! {
    "aut" => "author",
    "edt" => "editor",
    "trl" => "translator",
    "ill" => "illustrator",
};

#[derive(Debug, Deserialize)]
pub struct Package {
    pub metadata: Metadata,
    pub manifest: Manifest,
    pub spine: Spine,
}

impl Package {
    pub fn from_opf(opf: &mut ZipFile<File>) -> Result<Self> {
        let opf_reader = BufReader::new(opf);
        let package: Package = de::from_reader(opf_reader)?;
        Ok(package)
    }
}

#[derive(Debug, Deserialize)]
pub struct Metadata {
    pub title: Option<String>,
    #[serde(rename = "creator", default)]
    pub creators: Vec<Creator>,
    pub language: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "subject", default)]
    pub subjects: Vec<String>,
}

impl Metadata {
    pub fn write(&self, output_dir: &Path) -> Result<()> {
        let path = output_dir.join("metadata.toml");
        // 创建 TOML 文档
        let mut doc = DocumentMut::new();

        if let Some(title) = &self.title {
            doc["title"] = value(title.clone());
        }

        for creator in &self.creators {
            if let Some(role) = &creator.role {
                let role_key = ROLE_MAP.get(role.as_str()).unwrap_or(&"unknown");
                doc[role_key] = value(creator.name.clone());
            } else {
                doc["author"] = value(creator.name.clone());
            }
        }

        if let Some(language) = &self.language {
            doc["language"] = value(language.clone());
        }

        if let Some(description) = &self.description {
            doc["description"] = value(description.clone());
        }

        // 处理数组字段 subject
        if !self.subjects.is_empty() {
            let mut array = toml_edit::Array::new();
            for subject in &self.subjects {
                array.push(subject);
            }
            doc["subject"] = Item::Value(toml_edit::Value::Array(array));
        }

        // 写入文件
        fs::write(path, doc.to_string())?;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub struct Manifest {
    #[serde(rename = "item")]
    pub items: Vec<ManifestItem>,
}

impl Manifest {
    /// 转换为 HashMap<String, String> (id -> href)
    /// 过滤条件：
    /// 1. 排除 id 包含 "cover" 的项
    /// 2. 只保留 media_type 为 "application/xhtml+xml" 的项
    pub fn into_map(self) -> AHashMap<String, String> {
        self.items
            .into_iter()
            .filter(|item| !item.id.contains("cover") && item.media_type == "application/xhtml+xml")
            .map(|item| (item.id, item.href))
            .collect()
    }
}

#[derive(Debug, Deserialize)]
pub struct Spine {
    #[serde(rename = "itemref")]
    pub itemrefs: Vec<ItemRef>,
}

impl Spine {
    pub fn into_hrefs(self, mut id_href_map: AHashMap<String, String>) -> Vec<String> {
        self.itemrefs
            .into_iter()
            .filter_map(|itemref| id_href_map.remove(&itemref.idref))
            .collect()
    }
}

#[derive(Debug, Deserialize)]
pub struct ManifestItem {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "@href")]
    pub href: String,
    #[serde(rename = "@media-type")]
    pub media_type: String,
}

#[derive(Debug, Deserialize)]
pub struct ItemRef {
    #[serde(rename = "@idref")]
    pub idref: String,
}

#[derive(Debug, Deserialize)]
pub struct Creator {
    // $text获取元素和其子元素的文本内容
    // $value获取元素的文本内容
    #[serde(rename = "$value")]
    pub name: String,
    // @表示属性
    #[serde(rename = "@role")]
    pub role: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_extract_opf_path() {
        let opf = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="BookId" version="2.0">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
<dc:identifier id="BookId" opf:scheme="UUID">062b32e6-a657-42cf-95ba-5f9f6efd005a</dc:identifier>
<dc:language>ko</dc:language>
<dc:title>짝사랑했던 성녀의 딸을 주웠다</dc:title>
<dc:creator opf:role="aut">최태원씨</dc:creator>
<dc:description>걔한테 니엄마라고 해버렸다.</dc:description>
<meta name="cover" content="cover-image"/>
<dc:subject>판타지</dc:subject>
<dc:subject>전생</dc:subject>
<dc:subject>중세</dc:subject>
<dc:subject>모험</dc:subject>
<dc:subject>TS히로인</dc:subject>
<meta content="1.9.10" name="Sigil version"/>
</metadata>
<manifest>
<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
<item id="sgc-toc.css" href="Styles/sgc-toc.css" media-type="text/css"/>
<item id="Stylesheet.css" href="Styles/Stylesheet.css" media-type="text/css"/>
<item id="cover.html" href="Text/cover.html" media-type="application/xhtml+xml"/>
<item id="cover-image" href="Images/cover.jpg" media-type="image/jpeg"/>
<item id="chapternotice_0000.html" href="Text/chapternotice_0000.html" media-type="application/xhtml+xml"/>
<item id="chapternotice_0001.html" href="Text/chapternotice_0001.html" media-type="application/xhtml+xml"/>
<item id="chapternotice_0002.html" href="Text/chapternotice_0002.html" media-type="application/xhtml+xml"/>
</manifest>
<spine toc="ncx">
<itemref idref="cover.html"/>
<itemref idref="chapternotice_0000.html"/>
<itemref idref="chapternotice_0001.html"/>
<itemref idref="chapternotice_0002.html"/>
</spine>
</package>
    "#;
        let package: Package = quick_xml::de::from_str(opf).unwrap();
        println!("{:#?}", package);
    }
}
