use std::path::{Path, PathBuf};
use egui::ColorImage;
use std::process::Command;
use std::collections::{HashMap, HashSet};
use crossbeam_channel::{unbounded, Sender, Receiver};
use rayon::ThreadPoolBuilder;
use std::sync::{Arc, Mutex};
use file_icon_provider::get_file_icon;
use std::os::windows::process::CommandExt;

pub struct ThumbnailLoader {
    cache: HashMap<PathBuf, egui::TextureHandle>,
    tx: Sender<(PathBuf, Option<ColorImage>)>,
    rx: Receiver<(PathBuf, Option<ColorImage>)>,
    loading: Arc<Mutex<HashSet<PathBuf>>>,
    pool: rayon::ThreadPool,
}

impl ThumbnailLoader {
    pub fn new() -> Self {
        let (tx, rx) = unbounded();
        let pool = ThreadPoolBuilder::new().num_threads(8).build().unwrap();
        Self {
            cache: HashMap::new(),
            tx,
            rx,
            loading: Arc::new(Mutex::new(HashSet::new())),
            pool,
        }
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
        while let Ok(_) = self.rx.try_recv() {}
        self.loading.lock().unwrap().clear();
    }

    pub fn get(&mut self, path: &Path, ctx: &egui::Context, use_os_icons: bool) -> Option<egui::TextureHandle> {
        while let Ok((p, img_opt)) = self.rx.try_recv() {
            if let Some(img) = img_opt {
                let handle = ctx.load_texture(
                    p.to_string_lossy(),
                    img,
                    egui::TextureOptions::LINEAR
                );
                self.cache.insert(p.clone(), handle);
            }
            self.loading.lock().unwrap().remove(&p);
            ctx.request_repaint();
        }

        if let Some(handle) = self.cache.get(path) {
            return Some(handle.clone());
        }

        let mut loading = self.loading.lock().unwrap();
        if !loading.contains(path) {
            loading.insert(path.to_path_buf());
            let tx = self.tx.clone();
            let path_buf = path.to_path_buf();
            
            self.pool.spawn(move || {
                let ext = path_buf.extension().unwrap_or_default().to_ascii_lowercase();
                let ext_str = ext.to_string_lossy();
                let is_video = ["mp4", "avi", "mkv", "mov", "wmv", "flv"].contains(&ext_str.as_ref());
                
                let mut image: Option<image::RgbaImage> = None;

                if is_video {
                    let out_path = format!("temp_thumb_{}.png", path_buf.file_name().unwrap().to_string_lossy());
                    let _ = Command::new("ffmpeg")
                        .args(["-i", path_buf.to_str().unwrap(), "-vframes", "1", "-s", "256x256", &out_path, "-y"])
                        .creation_flags(0x08000000)
                        .output();
                    
                    if let Ok(img) = image::open(&out_path) {
                        image = Some(img.to_rgba8());
                        let _ = std::fs::remove_file(&out_path);
                    }
                } else if let Ok(img) = image::open(&path_buf) {
                    image = Some(img.thumbnail(256, 256).to_rgba8());
                } else if use_os_icons {
                    if let Ok(icon) = get_file_icon(&path_buf, 64) {
                        image = image::RgbaImage::from_raw(icon.width, icon.height, icon.pixels);
                    }
                }

                let color_img = image.map(|img| {
                    ColorImage::from_rgba_unmultiplied(
                        [img.width() as _, img.height() as _],
                        &img.into_raw(),
                    )
                });
                
                let _ = tx.send((path_buf, color_img));
            });
        }
        
        None
    }
}
