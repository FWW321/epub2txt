use std::fs::File;
use std::path::Path;
use std::io::{BufReader, Write};

use anyhow::Result;
use zip::ZipArchive;
use quick_xml::Reader;
use quick_xml::events::Event;

use crate::config::get_config;

#[derive(Debug)]
pub struct Chapter {
    pub title: String,

    pub content: String,
}

impl Chapter {
    pub fn extract_chapter(epub: &mut ZipArchive<File>, path: &str) -> Result<Chapter> {
        let file = epub.by_name(path)?;
        let reader = BufReader::new(file);
        let mut reader = Reader::from_reader(reader);
        reader.config_mut().trim_text(true);
        // 文本内容中是否应允许使用单独的 & 字符（不带与&成对的分号）
        reader.config_mut().allow_dangling_amp = true;
        // 自动扩展自闭合标签为开始和结束标签，会额外给end分配内存
        reader.config_mut().check_end_names = false;
        // check_end_names也会分配内存，如果两者同时开启，只会分配一次
        // check_end_names默认启用
        reader.config_mut().expand_empty_elements = true;

        let mut title = String::new();
        let mut content = String::with_capacity(800);
        let mut stack = Vec::new();
        let mut buf = Vec::with_capacity(800);

        loop {
            match reader.read_event_into(&mut buf)? {
                Event::Start(e) | Event::Empty(e) => {
                    stack.push(e.name().as_ref().to_vec());
                }
                Event::Text(text) => {
                    // html_content是xml10_content的别名，会自动处理实体转义，但是仅支持xml实体
                    // unescape 可以处理更多html实体
                    let decoded = text.html_content()?;

                    if let Some(tag) = stack.last() {
                        if get_config().tags.title.contains::<[u8]>(tag) {
                            title = decoded.into_owned();
                        } else if get_config().tags.inline.contains::<[u8]>(tag)
                            || get_config().tags.block.contains::<[u8]>(tag)
                        {
                            content.push_str(&decoded);
                        }
                    }
                }
                Event::End(e) => {
                    stack.pop();
                    let tag_bytes = e.name();

                    if get_config().tags.block.contains(tag_bytes.as_ref()) {
                        content.push('\n');
                    }
                }
                Event::Eof => break,
                _ => {}
            }
            buf.clear();
        }

        Ok(Chapter { title, content })
    }

    pub fn write(&self, output_dir: &Path, index: usize) -> Result<()> {
        let chapter_path = output_dir.join(format!("chapter_{}.txt", index));
        let mut file = File::create(chapter_path)?;

        writeln!(file, "{}\n", self.title)?;
        writeln!(file, "{}", self.content)?;
        Ok(())
    }
}

pub struct ChapterIter<'a> {
    archive: &'a mut ZipArchive<File>,
    paths: std::slice::Iter<'a, String>,
}

impl<'a> ChapterIter<'a> {
    pub fn new(
        archive: &'a mut ZipArchive<File>,
        paths: &'a [String],
    ) -> Self {
        Self {
            archive,
            paths: paths.iter(),
        }
    }
}

impl<'a> Iterator for ChapterIter<'a> {
    type Item = Result<Chapter>;

    fn next(&mut self) -> Option<Self::Item> {
        self.paths.next().map(|path| {
            Chapter::extract_chapter(self.archive, path)
        })
    }
}
