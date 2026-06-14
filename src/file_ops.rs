use std::path::{Path, PathBuf};
use trash;
use std::fs;
use fs_extra::dir::CopyOptions;

pub struct FileManager {
    pub clipboard: Vec<PathBuf>,
    pub action: ClipboardAction,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ClipboardAction {
    None,
    Copy,
    Cut,
}

impl Default for FileManager {
    fn default() -> Self {
        Self {
            clipboard: Vec::new(),
            action: ClipboardAction::None,
        }
    }
}

impl FileManager {
    pub fn copy_to_clipboard(&mut self, paths: Vec<PathBuf>) {
        self.clipboard = paths;
        self.action = ClipboardAction::Copy;
    }

    pub fn cut_to_clipboard(&mut self, paths: Vec<PathBuf>) {
        self.clipboard = paths;
        self.action = ClipboardAction::Cut;
    }

    pub fn paste(&mut self, target_dir: &Path) -> Result<String, String> {
        if self.clipboard.is_empty() {
            return Err("Clipboard is empty".into());
        }

        let mut success_count = 0;
        let mut errors = Vec::new();

        for src in &self.clipboard {
            let file_name = match src.file_name() {
                Some(n) => n,
                None => continue,
            };
            
            let mut dst = target_dir.join(file_name);
            
            // Handle name collision
            let mut counter = 1;
            while dst.exists() {
                let name = src.file_stem().unwrap_or_default().to_string_lossy();
                let ext = src.extension().unwrap_or_default().to_string_lossy();
                if ext.is_empty() {
                    dst = target_dir.join(format!("{} ({})", name, counter));
                } else {
                    dst = target_dir.join(format!("{} ({}).{}", name, counter, ext));
                }
                counter += 1;
            }

            let result = if self.action == ClipboardAction::Copy {
                if src.is_dir() {
                    let options = CopyOptions {
                        copy_inside: true,
                        ..Default::default()
                    };
                    // fs_extra copy requires source, target, and options
                    fs_extra::dir::copy(src, &dst, &options).map(|_| ()).map_err(|e| e.to_string())
                } else {
                    fs::copy(src, &dst).map(|_| ()).map_err(|e| e.to_string())
                }
            } else {
                fs::rename(src, &dst).map_err(|e| e.to_string())
            };

            match result {
                Ok(_) => success_count += 1,
                Err(e) => errors.push(format!("Failed {:?}: {}", src, e)),
            }
        }

        if self.action == ClipboardAction::Cut && success_count == self.clipboard.len() {
            self.clipboard.clear();
            self.action = ClipboardAction::None;
        }

        if errors.is_empty() {
            Ok("Paste successful".into())
        } else {
            Err(errors.join("\n"))
        }
    }

    pub fn delete(&self, paths: &[PathBuf]) -> Result<(), String> {
        trash::delete_all(paths).map_err(|e| e.to_string())
    }

    #[allow(dead_code)]
    pub fn rename(&self, old: &Path, new_name: &str) -> Result<(), String> {
        let new_path = old.with_file_name(new_name);
        if new_path.exists() {
            return Err("Name already exists".into());
        }
        fs::rename(old, new_path).map_err(|e| e.to_string())
    }
    
    pub fn create_folder(&self, target_dir: &Path, name: &str) -> Result<(), String> {
        let mut new_path = target_dir.join(name);
        let mut counter = 1;
        while new_path.exists() {
            new_path = target_dir.join(format!("{} ({})", name, counter));
            counter += 1;
        }
        fs::create_dir_all(new_path).map_err(|e| e.to_string())
    }
}
