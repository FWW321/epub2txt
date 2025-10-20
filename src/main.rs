use std::time::Instant;
use std::path::{Path, PathBuf};

use anyhow::Result;
use rayon::prelude::*;

use epub2txt::process;
use epub2txt::get_config;

fn main() -> Result<()> {
    let start = Instant::now();
    let input_dir = Path::new(&get_config().input_dir);
    if !(input_dir.exists() && input_dir.is_dir()) {
        anyhow::bail!("Input directory does not exist or is not a directory");
    }

    let tasks = get_tasks(input_dir)?;

    tasks
        .into_par_iter()
        .map(process_epub)
        .collect::<Result<Vec<()>>>()?;

    let duration = start.elapsed();

    display_elapsed_time(duration);

    Ok(())
}

fn process_epub(epub_path: PathBuf) -> anyhow::Result<()> {
    let mut epub = process::Epub::from_file(epub_path)?;
    epub.write()
}

fn get_tasks(input_dir: &Path) -> Result<Vec<PathBuf>> {
    let epub_paths: Vec<PathBuf> = input_dir
        .read_dir()?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            (path.extension()? == "epub").then_some(path)
        })
        .collect();

    Ok(epub_paths)
}

pub fn display_elapsed_time(duration: std::time::Duration) {
    let total_ms = duration.as_millis();

    if total_ms >= 60000 {
        // 超过1分钟：显示分秒
        let mins = total_ms / 60000;
        let secs = (total_ms % 60000) / 1000;
        let ms_remaining = total_ms % 1000;

        if ms_remaining > 0 {
            println!(
                "✅ 处理完成！耗时: {}分{}秒{}毫秒",
                mins, secs, ms_remaining
            );
        } else {
            println!("✅ 处理完成！耗时: {}分{}秒", mins, secs);
        }
    } else if total_ms >= 1000 {
        // 1秒到1分钟：显示秒和毫秒
        let secs = total_ms / 1000;
        let ms_remaining = total_ms % 1000;

        if ms_remaining > 0 {
            println!("✅ 处理完成！耗时: {}秒{}毫秒", secs, ms_remaining);
        } else {
            println!("✅ 处理完成！耗时: {}秒", secs);
        }
    } else {
        // 少于1秒：只显示毫秒
        println!("✅ 处理完成！耗时: {}毫秒", total_ms);
    }
}
