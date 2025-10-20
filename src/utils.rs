pub fn normalize_zip_path(opf_path: &str, rel: String) -> String {
    let mut result = String::with_capacity(opf_path.len() + rel.len());

    if let Some(pos) = opf_path.rfind('/') {
        result.push_str(&opf_path[..pos]);
    }

    // 处理相对路径部分
    for comp in rel.split('/') {
        match comp {
            ".." => {
                if let Some(pos) = result.rfind('/') {
                    result.truncate(pos);
                }
            },
            "." | "" => {}, // 忽略这两种情况
            _ => {
                if !result.is_empty() {
                    result.push('/');
                }
                result.push_str(comp);
            }
        }
    }

    println!("Normalized path: {}", result);

    result
}
