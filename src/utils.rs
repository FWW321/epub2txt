use std::path::{Component, Path, PathBuf};

pub fn normalize_zip_path(opf_path: impl AsRef<Path>, relative: impl AsRef<Path>) -> PathBuf {
    let opf_path = opf_path.as_ref();
    let relative = relative.as_ref();

    fn inner(opf_path: &Path, relative: &Path) -> PathBuf {
        let base = opf_path.parent().unwrap_or(Path::new(""));

        let mut result = base.to_path_buf();

        for component in relative.components() {
            match component {
                Component::ParentDir => {
                    result.pop();
                }
                Component::CurDir => {}
                _ => result.push(component),
            }
        }

        result
    }
    inner(opf_path, relative)
}
