use anyhow::Result;
use serde_json::{Value, json};
use std::path::PathBuf;

use super::Plugin;

pub struct FileBrowser {
    current_path: PathBuf,
    files: Vec<FileInfo>,
}

#[derive(Debug, Clone)]
struct FileInfo {
    name: String,
    is_directory: bool,
    size: u64,
    modified: String,
}

impl FileBrowser {
    pub fn new() -> Self {
        Self {
            current_path: PathBuf::from("/"),
            files: Vec::new(),
        }
    }

    pub fn set_path(&mut self, path: PathBuf) {
        self.current_path = path;
    }

    fn refresh_files(&mut self) -> Result<()> {
        self.files.clear();

        if let Ok(entries) = std::fs::read_dir(&self.current_path) {
            for entry in entries.flatten() {
                let metadata = entry.metadata()?;
                let name = entry.file_name().to_string_lossy().to_string();
                let modified = format!(
                    "{:?}",
                    metadata
                        .modified()
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                );

                self.files.push(FileInfo {
                    name,
                    is_directory: metadata.is_dir(),
                    size: metadata.len(),
                    modified,
                });
            }
        }

        // 排序：目录在前，然后按名称排序
        self.files
            .sort_by(|a, b| match (a.is_directory, b.is_directory) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            });

        Ok(())
    }
}

impl Plugin for FileBrowser {
    fn name(&self) -> &str {
        "File Browser"
    }

    fn is_enabled(&self) -> bool {
        true
    }

    async fn initialize(&mut self) -> Result<()> {
        self.refresh_files()?;
        Ok(())
    }

    async fn update(&mut self) -> Result<()> {
        self.refresh_files()?;
        Ok(())
    }

    fn render_data(&self) -> Value {
        let files: Vec<Value> = self
            .files
            .iter()
            .map(|file| {
                json!({
                    "name": file.name,
                    "is_directory": file.is_directory,
                    "size": file.size,
                    "modified": file.modified,
                    "type": if file.is_directory { "directory" } else { "file" }
                })
            })
            .collect();

        json!({
            "current_path": self.current_path.to_string_lossy(),
            "files": files,
            "file_count": self.files.len()
        })
    }
}
