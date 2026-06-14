use eframe::egui;
use std::path::PathBuf;
use std::fs;
use std::collections::HashSet;
use sysinfo::Disks;
use crate::thumbnail::ThumbnailLoader;
use crate::file_ops::FileManager;

#[derive(PartialEq)]
enum ViewMode {
    List,
    Grid,
}

#[derive(PartialEq)]
enum AppTheme {
    Light,
    Dark,
}

pub struct FileTreeItem {
    path: PathBuf,
    name: String,
    children: Option<Vec<FileTreeItem>>, // None = chưa load, Some = đã load
}

impl FileTreeItem {
    fn new(path: PathBuf, name: String) -> Self {
        Self {
            path,
            name,
            children: None,
        }
    }
}

pub struct MediaBrowserApp {
    current_dir: PathBuf,
    entries: Vec<PathBuf>,
    thumbnail_loader: ThumbnailLoader,
    file_manager: FileManager,
    view_mode: ViewMode,
    drives: Vec<FileTreeItem>,
    
    // Multi-select state
    selected_items: HashSet<PathBuf>,
    last_clicked_index: Option<usize>,
    
    // Drag-select state
    drag_start: Option<egui::Pos2>,
    drag_current: Option<egui::Pos2>,
    drag_selected_items: HashSet<PathBuf>,

    // Rename state
    renaming_path: Option<PathBuf>,
    rename_buffer: String,

    // Layout state
    show_preview: bool,
    path_input_buffer: String,
    
    // History state
    history: Vec<PathBuf>,
    history_index: usize,
    
    // Sidebar state
    last_navigated_dir: Option<PathBuf>,
    
    // Theme state
    theme: AppTheme,
    use_os_icons: bool,
}

impl Default for MediaBrowserApp {
    fn default() -> Self {
        let mut app = Self {
            current_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("C:\\")),
            entries: Vec::new(),
            thumbnail_loader: ThumbnailLoader::new(),
            file_manager: FileManager::default(),
            view_mode: ViewMode::Grid,
            drives: Vec::new(),
            selected_items: HashSet::new(),
            last_clicked_index: None,
            drag_start: None,
            drag_current: None,
            drag_selected_items: HashSet::new(),
            renaming_path: None,
            rename_buffer: String::new(),
            show_preview: false,
            path_input_buffer: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("C:\\")).to_string_lossy().to_string(),
            history: Vec::new(),
            history_index: 0,
            last_navigated_dir: None,
            theme: AppTheme::Light,
            use_os_icons: true,
        };
        
        app.refresh_drives();
        
        let start_dir = app.current_dir.clone();
        app.history.push(start_dir.clone());
        app.load_dir(start_dir);
        app
    }
}

impl MediaBrowserApp {
    fn invalidate_tree(&mut self) {
        for drive in &mut self.drives {
            drive.children = None;
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.renaming_path.is_some() {
            return;
        }

        // Bỏ qua shortcut nếu người dùng đang gõ văn bản (chẳng hạn thanh địa chỉ)
        if ctx.memory(|mem| mem.focused().is_some()) {
            return;
        }

        let mut refresh_needed = false;
        
        let mut do_copy = false;
        let mut do_cut = false;
        let mut do_paste = false;
        let mut do_delete = false;
        let mut do_select_all = false;

        ctx.input_mut(|i| {
            do_copy = i.modifiers.command && i.key_pressed(egui::Key::C);
            do_cut = i.modifiers.command && i.key_pressed(egui::Key::X);
            do_paste = i.modifiers.command && i.key_pressed(egui::Key::V);
            do_delete = i.key_pressed(egui::Key::Delete);
            do_select_all = i.modifiers.command && i.key_pressed(egui::Key::A);

            for e in &i.events {
                match e {
                    egui::Event::Copy => do_copy = true,
                    egui::Event::Cut => do_cut = true,
                    egui::Event::Paste(_) => do_paste = true,
                    _ => {}
                }
            }
        });

        if do_copy {
            let paths: Vec<PathBuf> = self.selected_items.iter().cloned().collect();
            if !paths.is_empty() {
                self.file_manager.copy_to_clipboard(paths);
            }
        }

        if do_cut {
            let paths: Vec<PathBuf> = self.selected_items.iter().cloned().collect();
            if !paths.is_empty() {
                self.file_manager.cut_to_clipboard(paths);
            }
        }

        if do_paste {
            let _ = self.file_manager.paste(&self.current_dir);
            refresh_needed = true;
        }

        if do_delete {
            let paths: Vec<PathBuf> = self.selected_items.iter().cloned().collect();
            if !paths.is_empty() {
                let _ = self.file_manager.delete(&paths);
                refresh_needed = true;
                self.selected_items.clear();
            }
        }
        
        if do_select_all {
            for entry in &self.entries {
                self.selected_items.insert(entry.clone());
            }
        }

        if refresh_needed {
            self.invalidate_tree();
            self.load_dir(self.current_dir.clone());
        }
    }

    fn refresh_drives(&mut self) {
        self.drives.clear();
        let mut seen = std::collections::HashSet::new();
        let disks = Disks::new_with_refreshed_list();
        for disk in disks.list() {
            let mount = disk.mount_point().to_path_buf();
            if seen.contains(&mount) {
                continue;
            }
            seen.insert(mount.clone());
            
            let mut name = disk.name().to_string_lossy().to_string();
            if name.is_empty() {
                name = "Local Disk".to_string();
            }
            let path_str = mount.to_string_lossy().to_string();
            let display_name = format!("{} ({})", name, path_str.trim_end_matches('\\'));
            self.drives.push(FileTreeItem::new(mount, display_name));
        }
    }

    fn navigate_to(&mut self, path: PathBuf) {
        if self.history.is_empty() || self.history[self.history_index] != path {
            self.history.truncate(self.history_index + 1);
            self.history.push(path.clone());
            self.history_index = self.history.len() - 1;
        }
        self.last_navigated_dir = Some(path.clone());
        self.load_dir(path);
    }

    fn load_dir(&mut self, path: PathBuf) {
        self.current_dir = path.clone();
        self.entries.clear();
        self.selected_items.clear();
        self.last_clicked_index = None;
        self.drag_start = None;
        self.drag_current = None;
        self.drag_selected_items.clear();
        self.path_input_buffer = path.to_string_lossy().to_string();
        
        if let Ok(read_dir) = fs::read_dir(path) {
            for entry in read_dir.flatten() {
                self.entries.push(entry.path());
            }
        }
        // Sắp xếp: Thư mục trước, file sau
        self.entries.sort_by(|a, b| {
            let a_is_dir = a.is_dir();
            let b_is_dir = b.is_dir();
            if a_is_dir && !b_is_dir {
                std::cmp::Ordering::Less
            } else if !a_is_dir && b_is_dir {
                std::cmp::Ordering::Greater
            } else {
                a.file_name().cmp(&b.file_name())
            }
        });
    }

    fn ui_left_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("left_panel")
            .resizable(true)
            .default_width(250.0)
            .show(ctx, |ui| {
                ui.heading("Duyệt thư mục");
                ui.add_space(5.0);
                
                let mut path_to_open = None;
                let mut refresh_needed = false;

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        enum TreeAction {
                            Click(PathBuf),
                            Copy(PathBuf),
                            Cut(PathBuf),
                            Delete(PathBuf),
                            StartRename(PathBuf, String),
                            Paste(PathBuf),
                            NewFolder(PathBuf),
                        }

                        // Đệ quy để vẽ cây thư mục
                        fn draw_tree(
                            ui: &mut egui::Ui,
                            item: &mut FileTreeItem,
                            current_dir: &PathBuf,
                            last_nav_dir: &Option<PathBuf>,
                            thumbnail_loader: &mut ThumbnailLoader,
                            use_os_icons: bool,
                        ) -> Option<TreeAction> {
                            ui.push_id(&item.path, |ui| {
                                let mut action = None;
                                
                                let id = ui.make_persistent_id(&item.path);
                                let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
                                
                                if let Some(nav_dir) = last_nav_dir {
                                    if nav_dir.starts_with(&item.path) && nav_dir != &item.path {
                                        state.set_open(true);
                                    }
                                }
                                
                                state.show_header(ui, |ui| {
                                        let is_selected = item.path == *current_dir;
                                        let mut clicked = false;
                                        
                                        let inner = ui.horizontal(|ui| {
                                            if let Some(texture) = thumbnail_loader.get(&item.path, ui.ctx(), use_os_icons) {
                                                ui.image((texture.id(), egui::vec2(16.0, 16.0)));
                                            } else {
                                                let icon = if item.path.parent().is_none() { "🖴" } else { "📁" };
                                                ui.label(icon);
                                            }
                                            let resp = ui.selectable_label(is_selected, &item.name);
                                            if resp.clicked() {
                                                clicked = true;
                                            }
                                            resp
                                        });
                                        
                                        let row_resp = inner.response.interact(egui::Sense::click());
                                        if row_resp.clicked() {
                                            clicked = true;
                                        }
                                        
                                        if clicked {
                                            action = Some(TreeAction::Click(item.path.clone()));
                                        }
                                        
                                        let mut menu_action = None;
                                        let mut show_menu = |ui: &mut egui::Ui| {
                                            if ui.button("Mở (Open)").clicked() {
                                                menu_action = Some(TreeAction::Click(item.path.clone()));
                                                ui.close_menu();
                                            }
                                            ui.separator();
                                            if ui.button("📁 Tạo thư mục mới (New Folder)").clicked() {
                                                menu_action = Some(TreeAction::NewFolder(item.path.clone()));
                                                ui.close_menu();
                                            }
                                            if ui.button("✂ Cắt (Cut)").clicked() {
                                                menu_action = Some(TreeAction::Cut(item.path.clone()));
                                                ui.close_menu();
                                            }
                                            if ui.button("📋 Sao chép (Copy)").clicked() {
                                                menu_action = Some(TreeAction::Copy(item.path.clone()));
                                                ui.close_menu();
                                            }
                                            if ui.button("📝 Dán (Paste)").clicked() {
                                                menu_action = Some(TreeAction::Paste(item.path.clone()));
                                                ui.close_menu();
                                            }
                                            if ui.button("✏ Đổi tên (Rename)").clicked() {
                                                menu_action = Some(TreeAction::StartRename(item.path.clone(), item.name.clone()));
                                                ui.close_menu();
                                            }
                                            if ui.button("🗑 Xóa (Delete)").clicked() {
                                                menu_action = Some(TreeAction::Delete(item.path.clone()));
                                                ui.close_menu();
                                            }
                                        };
                                        
                                        inner.inner.context_menu(&mut show_menu);
                                        row_resp.context_menu(&mut show_menu);
                                        
                                        if let Some(act) = menu_action {
                                            action = Some(act);
                                        }
                                    })
                                    .body(|ui| {
                                        // Lazy load children
                                        if item.children.is_none() {
                                            let mut children = Vec::new();
                                            if let Ok(entries) = std::fs::read_dir(&item.path) {
                                                for entry in entries.flatten() {
                                                    if let Ok(file_type) = entry.file_type() {
                                                        if file_type.is_dir() {
                                                            children.push(FileTreeItem::new(
                                                                entry.path(),
                                                                entry.file_name().to_string_lossy().to_string()
                                                            ));
                                                        }
                                                    }
                                                }
                                                // Sort folders alphabetically
                                                children.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                                            }
                                            item.children = Some(children);
                                        }
                                        
                                        // Vẽ các children
                                        if let Some(children) = &mut item.children {
                                            for child in children {
                                                if let Some(act) = draw_tree(ui, child, current_dir, last_nav_dir, thumbnail_loader, use_os_icons) {
                                                    action = Some(act);
                                                }
                                            }
                                        }
                                    });
                                
                                action
                            }).inner
                        }

                        let mut tree_action = None;
                        for drive in &mut self.drives {
                            if let Some(act) = draw_tree(ui, drive, &self.current_dir, &self.last_navigated_dir, &mut self.thumbnail_loader, self.use_os_icons) {
                                tree_action = Some(act);
                            }
                        }
                        
                        self.last_navigated_dir = None;

                        if let Some(action) = tree_action {
                            match action {
                                TreeAction::Click(p) => path_to_open = Some(p),
                                TreeAction::Copy(p) => self.file_manager.copy_to_clipboard(vec![p]),
                                TreeAction::Cut(p) => self.file_manager.cut_to_clipboard(vec![p]),
                                TreeAction::Paste(p) => { 
                                    let _ = self.file_manager.paste(&p); 
                                    refresh_needed = true;
                                },
                                TreeAction::Delete(p) => { 
                                    let _ = self.file_manager.delete(&[p]); 
                                    refresh_needed = true;
                                },
                                TreeAction::StartRename(p, name) => {
                                    self.renaming_path = Some(p);
                                    self.rename_buffer = name;
                                },
                                TreeAction::NewFolder(p) => {
                                    let _ = self.file_manager.create_folder(&p, "New Folder");
                                    refresh_needed = true;
                                }
                            }
                        }
                        
                        if refresh_needed {
                            self.invalidate_tree();
                        }
                    });

                if let Some(p) = path_to_open {
                    self.navigate_to(p);
                }
            });
    }

    fn ui_right_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut refresh_needed = false;
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.view_mode, ViewMode::List, "📄 List View");
                ui.selectable_value(&mut self.view_mode, ViewMode::Grid, "🔲 Grid View");
                
                ui.separator();
                let selected_paths: Vec<PathBuf> = self.selected_items.iter().cloned().collect();
                
                // Keyboard shortcuts (chỉ hoạt động khi không có ô text nào đang được nhập)
                if !ctx.wants_keyboard_input() {
                    if ui.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::CTRL, egui::Key::C))) {
                        if !selected_paths.is_empty() {
                            self.file_manager.copy_to_clipboard(selected_paths.clone());
                        }
                    }
                    if ui.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::CTRL, egui::Key::X))) {
                        if !selected_paths.is_empty() {
                            self.file_manager.cut_to_clipboard(selected_paths.clone());
                        }
                    }
                    if ui.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::CTRL, egui::Key::V))) {
                        if let Err(e) = self.file_manager.paste(&self.current_dir) {
                            println!("Paste error: {}", e);
                        }
                        refresh_needed = true;
                    }
                    if ui.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Delete))) {
                        if !selected_paths.is_empty() {
                            let _ = self.file_manager.delete(&selected_paths);
                            refresh_needed = true;
                        }
                    }
                }
                
                if ui.button("✂ Cut").clicked() {
                    if !selected_paths.is_empty() {
                        self.file_manager.cut_to_clipboard(selected_paths.clone());
                    }
                }
                
                if ui.button("📋 Copy").clicked() {
                    if !selected_paths.is_empty() {
                        self.file_manager.copy_to_clipboard(selected_paths.clone());
                    }
                }
                
                if ui.button("📝 Paste").clicked() {
                    if let Err(e) = self.file_manager.paste(&self.current_dir) {
                        println!("Paste error: {}", e);
                    }
                    refresh_needed = true;
                }
                if ui.button("🗑 Delete").clicked() {
                    if !selected_paths.is_empty() {
                        let _ = self.file_manager.delete(&selected_paths);
                        refresh_needed = true;
                    }
                }
                if ui.button("📁 New Folder").clicked() {
                    let _ = self.file_manager.create_folder(&self.current_dir, "New Folder");
                    refresh_needed = true;
                }
            });
            ui.separator();

            let mut path_to_open = None;
            let mut item_rects = Vec::new(); // Lưu bounding box để làm drag select

            egui::ScrollArea::vertical().show(ui, |ui| {
                if self.view_mode == ViewMode::Grid {
                    let item_size = egui::vec2(160.0, 190.0);
                    ui.horizontal_wrapped(|ui| {
                        for (index, path) in self.entries.iter().enumerate() {
                            let (rect, response) = ui.allocate_exact_size(item_size, egui::Sense::click());
                            item_rects.push((index, path.clone(), rect));
                            
                            if ui.is_rect_visible(rect) {
                                let name = path.file_name().unwrap_or_default().to_string_lossy();
                                let is_dir = path.is_dir();
                                let is_selected = self.selected_items.contains(path) || self.drag_selected_items.contains(path);
                                
                                // Nền khi chọn hoặc di chuột
                                if is_selected {
                                    ui.painter().rect_filled(rect, 4.0, ui.visuals().selection.bg_fill);
                                } else if response.hovered() {
                                    ui.painter().rect_filled(rect, 4.0, ui.visuals().widgets.hovered.bg_fill);
                                }
                                
                                // Vùng Icon
                                let icon_rect = egui::Rect::from_center_size(
                                    rect.center() - egui::vec2(0.0, 20.0),
                                    egui::vec2(128.0, 128.0),
                                );
                                
                                if let Some(texture) = self.thumbnail_loader.get(path, ctx, self.use_os_icons) {
                                    let size = texture.size_vec2();
                                    let ratio = size.x / size.y;
                                    let draw_size = if ratio > 1.0 {
                                        egui::vec2(128.0, 128.0 / ratio)
                                    } else {
                                        egui::vec2(128.0 * ratio, 128.0)
                                    };
                                    
                                    ui.painter().image(
                                        texture.id(),
                                        egui::Rect::from_center_size(icon_rect.center(), draw_size),
                                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                        egui::Color32::WHITE,
                                    );
                                } else {
                                    let icon = if is_dir { "📁" } else { "📄" };
                                    ui.painter().text(
                                        icon_rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        icon,
                                        egui::FontId::proportional(64.0),
                                        if is_selected { ui.visuals().selection.stroke.color } else { ui.visuals().text_color() },
                                    );
                                }

                                // Vùng Text
                                let text_rect = egui::Rect::from_min_max(
                                    egui::pos2(rect.min.x, rect.max.y - 40.0),
                                    rect.max,
                                );
                                
                                let galley = ui.painter().layout(
                                    name.to_string(),
                                    egui::FontId::proportional(14.0),
                                    if is_selected { ui.visuals().selection.stroke.color } else { ui.visuals().text_color() },
                                    rect.width() - 8.0,
                                );
                                
                                ui.painter().galley(
                                    egui::pos2(text_rect.center().x - galley.size().x / 2.0, text_rect.min.y),
                                    galley,
                                    if is_selected { ui.visuals().selection.stroke.color } else { ui.visuals().text_color() }
                                );
                                
                                // Xử lý Click Multi-Select
                                if response.clicked() {
                                    let modifiers = ui.input(|i| i.modifiers);
                                    if modifiers.ctrl {
                                        if self.selected_items.contains(path) {
                                            self.selected_items.remove(path);
                                        } else {
                                            self.selected_items.insert(path.clone());
                                        }
                                        self.last_clicked_index = Some(index);
                                    } else if modifiers.shift {
                                        if let Some(last_idx) = self.last_clicked_index {
                                            let start = last_idx.min(index);
                                            let end = last_idx.max(index);
                                            for i in start..=end {
                                                self.selected_items.insert(self.entries[i].clone());
                                            }
                                        }
                                    } else {
                                        self.selected_items.clear();
                                        self.selected_items.insert(path.clone());
                                        self.last_clicked_index = Some(index);
                                    }
                                }

                                if response.double_clicked() {
                                    if is_dir {
                                        path_to_open = Some(path.clone());
                                    } else {
                                        let _ = std::process::Command::new("explorer").arg(path).spawn();
                                    }
                                }
                                
                                response.context_menu(|ui| {
                                    if !self.selected_items.contains(path) {
                                        self.selected_items.clear();
                                        self.selected_items.insert(path.clone());
                                    }
                                    let selected_paths: Vec<PathBuf> = self.selected_items.iter().cloned().collect();

                                    if ui.button("Mở (Open)").clicked() {
                                        if is_dir {
                                            path_to_open = Some(path.clone());
                                        } else {
                                            let _ = std::process::Command::new("explorer").arg(path).spawn();
                                        }
                                        ui.close_menu();
                                    }
                                    ui.separator();
                                    if ui.button("✂ Cắt (Cut)").clicked() {
                                        self.file_manager.cut_to_clipboard(selected_paths.clone());
                                        ui.close_menu();
                                    }
                                    if ui.button("📋 Sao chép (Copy)").clicked() {
                                        self.file_manager.copy_to_clipboard(selected_paths.clone());
                                        ui.close_menu();
                                    }
                                    if ui.button("✏ Đổi tên (Rename)").clicked() {
                                        if selected_paths.len() == 1 {
                                            self.renaming_path = Some(selected_paths[0].clone());
                                            self.rename_buffer = selected_paths[0].file_name().unwrap_or_default().to_string_lossy().to_string();
                                        }
                                        ui.close_menu();
                                    }
                                    if ui.button("🗑 Xóa (Delete)").clicked() {
                                        let _ = self.file_manager.delete(&selected_paths);
                                        refresh_needed = true;
                                        ui.close_menu();
                                    }
                                    ui.separator();
                                    if ui.button("ℹ Thuộc tính (Properties)").clicked() {
                                        // Menu không đóng để đọc thông tin
                                    }
                                    let mut total_size = 0;
                                    for p in &selected_paths {
                                        if let Ok(meta) = std::fs::metadata(p) {
                                            total_size += meta.len();
                                        }
                                    }
                                    let size_str = if total_size > 1024 * 1024 {
                                        format!("{} MB", total_size / (1024 * 1024))
                                    } else {
                                        format!("{} KB", total_size / 1024)
                                    };
                                    ui.add_enabled(false, egui::Button::new(format!("Đã chọn: {} mục", selected_paths.len())));
                                    ui.add_enabled(false, egui::Button::new(format!("Dung lượng: {}", size_str)));
                                });
                            }
                        }
                    });
                } else {
                    use egui_extras::{TableBuilder, Column};
                    
                    TableBuilder::new(ui)
                        .striped(true)
                        .resizable(true)
                        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                        .column(Column::initial(400.0).at_least(100.0).clip(true))
                        .column(Column::initial(100.0))
                        .column(Column::initial(100.0))
                        .header(25.0, |mut header| {
                            header.col(|ui| { ui.heading("Name"); });
                            header.col(|ui| { ui.heading("Size"); });
                            header.col(|ui| { ui.heading("Type"); });
                        })
                        .body(|mut body| {
                            for (index, path) in self.entries.iter().enumerate() {
                                let is_selected = self.selected_items.contains(path) || self.drag_selected_items.contains(path);
                                
                                body.row(25.0, |mut row| {
                                    row.set_selected(is_selected);
                                    let is_dir = path.is_dir();
                                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                                    
                                    row.col(|ui| {
                                        let mut r_rect = None;
                                        let inner = ui.horizontal(|ui| {
                                            if let Some(texture) = self.thumbnail_loader.get(path, ui.ctx(), self.use_os_icons) {
                                                ui.image((texture.id(), egui::vec2(16.0, 16.0)));
                                            } else {
                                                let icon = if is_dir { "📁" } else { "📄" };
                                                ui.label(icon);
                                            }
                                            
                                            let resp = ui.selectable_label(is_selected, name.to_string());
                                            r_rect = Some(resp);
                                        });
                                        
                                        let row_resp = inner.response.interact(egui::Sense::click());
                                        let resp = r_rect.unwrap();
                                        
                                        item_rects.push((index, path.clone(), row_resp.rect));
                                        
                                        if resp.clicked() || row_resp.clicked() {
                                            let modifiers = ui.input(|i| i.modifiers);
                                            if modifiers.ctrl {
                                                if self.selected_items.contains(path) {
                                                    self.selected_items.remove(path);
                                                } else {
                                                    self.selected_items.insert(path.clone());
                                                }
                                                self.last_clicked_index = Some(index);
                                            } else if modifiers.shift {
                                                if let Some(last_idx) = self.last_clicked_index {
                                                    let start = last_idx.min(index);
                                                    let end = last_idx.max(index);
                                                    for i in start..=end {
                                                        self.selected_items.insert(self.entries[i].clone());
                                                    }
                                                }
                                            } else {
                                                self.selected_items.clear();
                                                self.selected_items.insert(path.clone());
                                                self.last_clicked_index = Some(index);
                                            }
                                        }

                                        if resp.double_clicked() {
                                            if is_dir {
                                                path_to_open = Some(path.clone());
                                            } else {
                                                let _ = std::process::Command::new("explorer").arg(path).spawn();
                                            }
                                        }
                                        
                                        resp.context_menu(|ui| {
                                            if !self.selected_items.contains(path) {
                                                self.selected_items.clear();
                                                self.selected_items.insert(path.clone());
                                            }
                                            let selected_paths: Vec<PathBuf> = self.selected_items.iter().cloned().collect();

                                            if ui.button("Mở (Open)").clicked() {
                                                if is_dir {
                                                    path_to_open = Some(path.clone());
                                                } else {
                                                    let _ = std::process::Command::new("explorer").arg(path).spawn();
                                                }
                                                ui.close_menu();
                                            }
                                            ui.separator();
                                            if ui.button("✂ Cắt (Cut)").clicked() {
                                                self.file_manager.cut_to_clipboard(selected_paths.clone());
                                                ui.close_menu();
                                            }
                                            if ui.button("📋 Sao chép (Copy)").clicked() {
                                                self.file_manager.copy_to_clipboard(selected_paths.clone());
                                                ui.close_menu();
                                            }
                                            if ui.button("✏ Đổi tên (Rename)").clicked() {
                                                if selected_paths.len() == 1 {
                                                    self.renaming_path = Some(selected_paths[0].clone());
                                                    self.rename_buffer = selected_paths[0].file_name().unwrap_or_default().to_string_lossy().to_string();
                                                }
                                                ui.close_menu();
                                            }
                                            if ui.button("🗑 Xóa (Delete)").clicked() {
                                                let _ = self.file_manager.delete(&selected_paths);
                                                refresh_needed = true;
                                                ui.close_menu();
                                            }
                                            ui.separator();
                                            if ui.button("ℹ Thuộc tính (Properties)").clicked() {
                                                // Menu không đóng để đọc thông tin
                                            }
                                            let mut total_size = 0;
                                            for p in &selected_paths {
                                                if let Ok(meta) = std::fs::metadata(p) {
                                                    total_size += meta.len();
                                                }
                                            }
                                            let size_str = if total_size > 1024 * 1024 {
                                                format!("{} MB", total_size / (1024 * 1024))
                                            } else {
                                                format!("{} KB", total_size / 1024)
                                            };
                                            ui.add_enabled(false, egui::Button::new(format!("Đã chọn: {} mục", selected_paths.len())));
                                            ui.add_enabled(false, egui::Button::new(format!("Dung lượng: {}", size_str)));
                                        });
                                    });
                                    
                                    row.col(|ui| {
                                        if !is_dir {
                                            if let Ok(meta) = std::fs::metadata(path) {
                                                let kb = meta.len() / 1024;
                                                if kb > 1024 {
                                                    ui.label(format!("{} MB", kb / 1024));
                                                } else {
                                                    ui.label(format!("{} KB", kb));
                                                }
                                            }
                                        }
                                    });
                                    
                                    row.col(|ui| {
                                        if is_dir {
                                            ui.label("Folder");
                                        } else {
                                            let ext = path.extension().unwrap_or_default().to_string_lossy();
                                            ui.label(format!("{} File", ext.to_uppercase()));
                                        }
                                    });
                                });
                            }
                        });
                }
            });

            // Logic xử lý kéo quét chuột (Rubber band)
            let is_mouse_down = ctx.input(|i| i.pointer.primary_down());
            let is_mouse_pressed = ctx.input(|i| i.pointer.primary_pressed());
            let wants_pointer = ctx.wants_pointer_input();

            if is_mouse_pressed && !wants_pointer {
                // Click vào vùng trống
                if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
                    self.drag_start = Some(pos);
                    self.drag_selected_items.clear();
                    if !ctx.input(|i| i.modifiers.ctrl) {
                        self.selected_items.clear();
                    }
                }
            }

            if is_mouse_down {
                if self.drag_start.is_some() {
                    if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
                        self.drag_current = Some(pos);
                        if let (Some(start), Some(curr)) = (self.drag_start, self.drag_current) {
                            let drag_rect = egui::Rect::from_two_pos(start, curr);
                            self.drag_selected_items.clear();
                            for (_idx, path, rect) in &item_rects {
                                if drag_rect.intersects(*rect) {
                                    self.drag_selected_items.insert(path.clone());
                                }
                            }
                        }
                    }
                }
            } else {
                // Chuột nhả ra
                if self.drag_start.is_some() {
                    for p in self.drag_selected_items.drain() {
                        self.selected_items.insert(p);
                    }
                    self.drag_start = None;
                    self.drag_current = None;
                }
            }

            // Vẽ khung màu xanh dương (Rubber band) khi đang kéo
            if let (Some(start), Some(curr)) = (self.drag_start, self.drag_current) {
                let rect = egui::Rect::from_two_pos(start, curr);
                ui.painter().rect_filled(rect, 0.0, egui::Color32::from_rgba_unmultiplied(0, 150, 255, 30));
                ui.painter().rect_stroke(rect, 0.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(0, 150, 255)));
            }

            let mut do_rename = false;
            let mut close_rename = false;

            if let Some(path) = &self.renaming_path {
                egui::Window::new("Đổi tên (Rename)")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(format!("Đổi tên cho: {}", path.file_name().unwrap_or_default().to_string_lossy()));
                        let response = ui.text_edit_singleline(&mut self.rename_buffer);
                        response.request_focus();

                        let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
                        let esc_pressed = ui.input(|i| i.key_pressed(egui::Key::Escape));

                        ui.horizontal(|ui| {
                            if ui.button("Lưu (Enter)").clicked() || enter_pressed {
                                do_rename = true;
                            }
                            if ui.button("Hủy (Esc)").clicked() || esc_pressed {
                                close_rename = true;
                            }
                        });
                    });
            }

            if do_rename {
                if let Some(path) = self.renaming_path.take() {
                    if !self.rename_buffer.is_empty() {
                        let _ = self.file_manager.rename(&path, &self.rename_buffer);
                        refresh_needed = true;
                    }
                }
            } else if close_rename {
                self.renaming_path = None;
            }

            if let Some(p) = path_to_open {
                self.navigate_to(p);
            } else if refresh_needed {
                self.invalidate_tree();
                self.load_dir(self.current_dir.clone());
            }
        });
    }
}

impl eframe::App for MediaBrowserApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_shortcuts(ctx);
        
        // Áp dụng theme
        if self.theme == AppTheme::Light {
            ctx.set_visuals(egui::Visuals::light());
        } else {
            ctx.set_visuals(egui::Visuals::dark());
        }

        // TOP PANEL (Thanh địa chỉ)
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let can_go_back = self.history_index > 0;
                if ui.add_enabled(can_go_back, egui::Button::new("⬅")).clicked() {
                    self.history_index -= 1;
                    let p = self.history[self.history_index].clone();
                    self.load_dir(p);
                }
                
                let can_go_forward = self.history_index + 1 < self.history.len();
                if ui.add_enabled(can_go_forward, egui::Button::new("➡")).clicked() {
                    self.history_index += 1;
                    let p = self.history[self.history_index].clone();
                    self.load_dir(p);
                }
                
                let parent = self.current_dir.parent().map(|p| p.to_path_buf());
                if ui.add_enabled(parent.is_some(), egui::Button::new("⬆")).clicked() {
                    if let Some(p) = parent {
                        self.navigate_to(p);
                    }
                }
                
                ui.separator();
                ui.label("Đường dẫn:");
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let preview_text = if self.show_preview { "👁 Ẩn Preview" } else { "👁 Hiện Preview" };
                    if ui.button(preview_text).clicked() {
                        self.show_preview = !self.show_preview;
                    }
                    
                    ui.separator();
                    
                    if ui.checkbox(&mut self.use_os_icons, "Icon HĐH").changed() {
                        self.thumbnail_loader.clear_cache();
                        self.invalidate_tree();
                    }
                    
                    ui.separator();
                    
                    ui.selectable_value(&mut self.theme, AppTheme::Dark, "🌙 Tối");
                    ui.selectable_value(&mut self.theme, AppTheme::Light, "🌞 Sáng");
                    ui.label("Giao diện:");
                    
                    ui.separator();
                    
                    let mut go_clicked = ui.button("🚀 Go to").clicked();
                    
                    // Address bar takes the remaining space
                    let res = ui.add(egui::TextEdit::singleline(&mut self.path_input_buffer).desired_width(ui.available_width()));
                    
                    if res.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        go_clicked = true;
                    }
                    
                    if go_clicked {
                        let new_path = PathBuf::from(&self.path_input_buffer);
                        if new_path.exists() {
                            self.navigate_to(new_path);
                        }
                    }
                });
            });
        });

        // BOTTOM PANEL (Thanh trạng thái)
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Tổng số mục: {}", self.entries.len()));
                ui.separator();
                
                let selected_paths: Vec<PathBuf> = self.selected_items.iter().cloned().collect();
                if selected_paths.is_empty() {
                    ui.label("Chưa chọn mục nào.");
                } else {
                    let mut total_size = 0;
                    let mut folders = 0;
                    let mut files = 0;
                    for p in &selected_paths {
                        if p.is_dir() {
                            folders += 1;
                        } else {
                            files += 1;
                            if let Ok(meta) = std::fs::metadata(p) {
                                total_size += meta.len();
                            }
                        }
                    }
                    
                    let size_str = if total_size > 1024 * 1024 * 1024 {
                        format!("{:.2} GB", total_size as f64 / (1024.0 * 1024.0 * 1024.0))
                    } else if total_size > 1024 * 1024 {
                        format!("{:.2} MB", total_size as f64 / (1024.0 * 1024.0))
                    } else {
                        format!("{} KB", total_size / 1024)
                    };
                    
                    ui.label(format!("Đã chọn: {} mục ({} thư mục, {} file) | Dung lượng: {}", selected_paths.len(), folders, files, size_str));
                }
            });
        });

        // RIGHT PANEL (Khung Preview)
        if self.show_preview {
            egui::SidePanel::right("right_preview_panel")
                .resizable(true)
                .default_width(300.0)
                .show(ctx, |ui| {
                    ui.heading("Khung xem trước");
                    ui.separator();
                    
                    let selected_paths: Vec<PathBuf> = self.selected_items.iter().cloned().collect();
                    if selected_paths.len() == 1 {
                        let path = &selected_paths[0];
                        if path.is_dir() {
                            ui.label("Thư mục:");
                            ui.heading(path.file_name().unwrap_or_default().to_string_lossy());
                        } else {
                            ui.label("File:");
                            ui.heading(path.file_name().unwrap_or_default().to_string_lossy());
                            ui.add_space(10.0);
                            
                            // Hiển thị ảnh preview
                            if let Some(texture) = self.thumbnail_loader.get(path, ctx, self.use_os_icons) {
                                let size = texture.size_vec2();
                                let ratio = size.x / size.y;
                                
                                let avail_width = ui.available_width();
                                let draw_width = avail_width;
                                let draw_height = avail_width / ratio;
                                
                                ui.image((texture.id(), egui::vec2(draw_width, draw_height)));
                            } else {
                                ui.label("(Đang tải hoặc file không hỗ trợ preview)");
                            }
                        }
                    } else if selected_paths.is_empty() {
                        ui.label("Hãy click chọn một file để xem trước.");
                    } else {
                        ui.label(format!("Đang chọn {} mục. Chỉ preview khi chọn 1 file.", selected_paths.len()));
                    }
                });
        }

        self.ui_left_panel(ctx);
        self.ui_right_panel(ctx);
    }
}
