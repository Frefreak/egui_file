use std::cmp::{max, min};
use std::{
  env,
  fmt::Debug,
  fs,
  io::Error,
  ops::Deref,
  path::{Path, PathBuf},
};

use egui::{
  vec2, Align2, Context, Id, Key, Layout, Pos2, RichText, ScrollArea, TextEdit, Ui, Vec2, Window,
};

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
/// Dialog state.
pub enum State {
  /// Is currently visible.
  Open,
  /// Is currently not visible.
  Closed,
  /// Was canceled.
  Cancelled,
  /// File was selected.
  Selected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Dialog type.
pub enum DialogType {
  SelectFolder,
  OpenFile,
  SaveFile,
}

/// `egui` component that represents `OpenFileDialog` or `SaveFileDialog`.
pub struct FileDialog {
  /// Current opened path.
  path: PathBuf,

  /// Editable field with path.
  path_edit: String,

  /// Selected file path (single select mode).
  selected_file: Option<FileInfo>,

  /// Editable field with filename.
  filename_edit: String,

  /// Dialog title text
  title: String,

  /// Files in directory.
  files: Result<Vec<FileInfo>, Error>,

  /// Current dialog state.
  state: State,

  /// Dialog type.
  dialog_type: DialogType,

  id: Option<Id>,
  current_pos: Option<Pos2>,
  default_size: Vec2,
  anchor: Option<(Align2, Vec2)>,
  show_files_filter: Filter<PathBuf>,
  filename_filter: Filter<String>,
  resizable: bool,
  rename: bool,
  new_folder: bool,
  multi_select_enabled: bool,
  range_start: Option<usize>,

  /// Show drive letters on Windows.
  #[cfg(windows)]
  show_drives: bool,

  /// Show hidden files on unix systems.
  #[cfg(unix)]
  show_hidden: bool,
}

impl Debug for FileDialog {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let mut dbg = f.debug_struct("FileDialog");
    let dbg = dbg
      .field("path", &self.path)
      .field("path_edit", &self.path_edit)
      .field("selected_file", &self.selected_file)
      .field("filename_edit", &self.filename_edit)
      .field("files", &self.files)
      .field("state", &self.state)
      .field("dialog_type", &self.dialog_type)
      .field("current_pos", &self.current_pos)
      .field("default_size", &self.default_size)
      .field("anchor", &self.anchor)
      .field("resizable", &self.resizable)
      .field("rename", &self.rename)
      .field("new_folder", &self.new_folder)
      .field("multi_select", &self.multi_select_enabled)
      .field("range_start", &self.range_start);

    // Closures don't implement std::fmt::Debug.
    // .field("shown_files_filter", &self.shown_files_filter)
    // .field("filename_filter", &self.filename_filter)

    #[cfg(unix)]
    let dbg = dbg.field("show_hidden", &self.show_hidden);

    #[cfg(windows)]
    let dbg = dbg.field("show_drives", &self.show_drives);

    dbg.finish()
  }
}

/// Function that returns `true` if the path is accepted.
pub type Filter<T> = Box<dyn Fn(&<T as Deref>::Target) -> bool + Send + Sync + 'static>;

impl FileDialog {
  /// Create dialog that prompts the user to select a folder.
  pub fn select_folder(initial_path: Option<PathBuf>) -> Self {
    FileDialog::new(DialogType::SelectFolder, initial_path)
  }

  /// Create dialog that prompts the user to open a file.
  pub fn open_file(initial_path: Option<PathBuf>) -> Self {
    FileDialog::new(DialogType::OpenFile, initial_path)
  }

  /// Create dialog that prompts the user to save a file.
  pub fn save_file(initial_path: Option<PathBuf>) -> Self {
    FileDialog::new(DialogType::SaveFile, initial_path)
  }

  /// Constructs new file dialog. If no `initial_path` is passed,`env::current_dir` is used.
  fn new(dialog_type: DialogType, initial_path: Option<PathBuf>) -> Self {
    let mut path = initial_path.unwrap_or_else(|| env::current_dir().unwrap_or_default());
    let mut filename_edit = String::new();

    if path.is_file() {
      assert!(dialog_type != DialogType::SelectFolder);

      let info = FileInfo {
        path: path.clone(),
        dir: false,
        selected: false,
      };

      filename_edit = get_file_name(&info).to_string();
      path.pop();
    }

    let path_edit = path.to_str().unwrap_or_default().to_string();

    Self {
      path,
      path_edit,
      selected_file: None,
      filename_edit,
      title: match dialog_type {
        DialogType::SelectFolder => "📁  Select Folder",
        DialogType::OpenFile => "📂  Open File",
        DialogType::SaveFile => "💾  Save File",
      }
      .to_string(),
      files: Ok(Vec::new()),
      state: State::Closed,
      dialog_type,

      id: None,
      current_pos: None,
      default_size: vec2(512.0, 512.0),
      anchor: None,
      show_files_filter: Box::new(|_| true),
      filename_filter: Box::new(|_| true),
      resizable: true,
      rename: true,
      new_folder: true,

      #[cfg(windows)]
      show_drives: true,

      #[cfg(unix)]
      show_hidden: false,
      multi_select_enabled: false,
      range_start: None,
    }
  }

  /// Set the default file name.
  pub fn default_filename(mut self, filename: impl Into<String>) -> Self {
    self.filename_edit = filename.into();
    self
  }

  /// Set the window title text.
  pub fn title(mut self, title: &str) -> Self {
    self.title = match self.dialog_type {
      DialogType::SelectFolder => "📁  ",
      DialogType::OpenFile => "📂  ",
      DialogType::SaveFile => "💾  ",
    }
    .to_string()
      + title;
    self
  }

  /// Set the window ID.
  pub fn id(mut self, id: impl Into<Id>) -> Self {
    self.id = Some(id.into());
    self
  }

  /// Set the window anchor.
  pub fn anchor(mut self, align: Align2, offset: impl Into<Vec2>) -> Self {
    self.anchor = Some((align, offset.into()));
    self
  }

  /// Set the window position.
  pub fn current_pos(mut self, current_pos: impl Into<Pos2>) -> Self {
    self.current_pos = Some(current_pos.into());
    self
  }

  /// Set the window default size.
  pub fn default_size(mut self, default_size: impl Into<Vec2>) -> Self {
    self.default_size = default_size.into();
    self
  }

  /// Enable/disable resizing the window. Default is `true`.
  pub fn resizable(mut self, resizable: bool) -> Self {
    self.resizable = resizable;
    self
  }

  /// Show the Rename button. Default is `true`.
  pub fn show_rename(mut self, rename: bool) -> Self {
    self.rename = rename;
    self
  }

  /// Show the New Folder button. Default is `true`.
  pub fn show_new_folder(mut self, new_folder: bool) -> Self {
    self.new_folder = new_folder;
    self
  }

  pub fn multi_select(mut self, multi_select: bool) -> Self {
    self.multi_select_enabled = multi_select;
    self
  }

  pub fn has_multi_select(&self) -> bool {
    self.multi_select_enabled
  }
  /// Show the mapped drives on Windows. Default is `true`.
  #[cfg(windows)]
  pub fn show_drives(mut self, drives: bool) -> Self {
    self.show_drives = drives;
    self
  }

  /// Set a function to filter listed files.
  pub fn show_files_filter(mut self, filter: Filter<PathBuf>) -> Self {
    self.show_files_filter = filter;
    self
  }

  /// Set a function to filter the selected filename.
  pub fn filename_filter(mut self, filter: Filter<String>) -> Self {
    self.filename_filter = filter;
    self
  }

  /// Get the dialog type.
  pub fn dialog_type(&self) -> DialogType {
    self.dialog_type
  }

  /// Get the window's visibility.
  pub fn visible(&self) -> bool {
    self.state == State::Open
  }

  /// Opens the dialog.
  pub fn open(&mut self) {
    self.state = State::Open;
    self.refresh();
  }

  /// Resulting file path.
  pub fn path(&self) -> Option<&Path> {
    self.selected_file.as_ref().map(|info| info.path.as_path())
  }

  /// Retrieves multi selection as a vector.
  pub fn selection(&self) -> Vec<&Path> {
    match self.files {
      Ok(ref files) => files
        .iter()
        .filter_map(|info| {
          if info.selected {
            Some(info.path.as_path())
          } else {
            None
          }
        })
        .collect(),
      Err(_) => Vec::new(),
    }
  }

  /// Currently mounted directory that is being shown in the dialog box
  pub fn directory(&self) -> &Path {
    self.path.as_path()
  }

  /// Set the dialog's current opened path
  pub fn set_path(&mut self, path: impl Into<PathBuf>) {
    self.path = path.into();
    self.refresh();
  }

  /// Dialog state.
  pub fn state(&self) -> State {
    self.state
  }

  /// Returns true, if the file selection was confirmed.
  pub fn selected(&self) -> bool {
    self.state == State::Selected
  }

  fn open_selected(&mut self) {
    if let Some(info) = &self.selected_file {
      if info.dir {
        self.set_path(info.path.clone())
      } else if self.dialog_type == DialogType::OpenFile {
        self.confirm();
      }
    } else if self.multi_select_enabled && self.dialog_type == DialogType::OpenFile {
      self.confirm();
    }
  }

  fn confirm(&mut self) {
    self.state = State::Selected;
  }

  fn refresh(&mut self) {
    self.files = self.read_folder();
    self.path_edit = String::from(self.path.to_str().unwrap_or_default());
    self.select(None);
    self.selected_file = None;
  }

  fn select(&mut self, file: Option<FileInfo>) {
    if let Some(info) = &file {
      self.filename_edit = get_file_name(info).to_owned();
    }
    self.selected_file = file;
  }

  fn select_reset_multi(&mut self, idx: usize) {
    if let Ok(files) = &mut self.files {
      let selected_val = files[idx].selected;
      for file in files.iter_mut() {
        file.selected = false;
      }
      files[idx].selected = !selected_val;
      self.range_start = Some(idx);
    }
  }

  fn select_switch_multi(&mut self, idx: usize) {
    if let Ok(files) = &mut self.files {
      files[idx].selected = !files[idx].selected;
      if files[idx].selected {
        self.range_start = Some(idx);
      } else {
        self.range_start = None;
      }
    } else {
      self.range_start = None;
    }
  }

  fn select_range(&mut self, idx: usize) {
    if let Ok(files) = &mut self.files {
      if let Some(range_start) = self.range_start {
        let range = min(idx, range_start)..=max(idx, range_start);
        for i in range {
          files[i].selected = true;
        }
      }
    }
  }

  fn can_save(&self) -> bool {
    !self.filename_edit.is_empty() && (self.filename_filter)(self.filename_edit.as_str())
  }

  fn can_open(&self) -> bool {
    if self.multi_select_enabled {
      if let Ok(files) = &self.files {
        for file in files {
          if file.selected && (self.filename_filter)(get_file_name(file)) {
            return true;
          }
        }
      }
      false
    } else {
      !self.filename_edit.is_empty() && (self.filename_filter)(self.filename_edit.as_str())
    }
  }

  fn can_rename(&self) -> bool {
    if !self.filename_edit.is_empty() {
      if let Some(file) = &self.selected_file {
        return get_file_name(file) != self.filename_edit;
      }
    }
    false
  }

  /// Shows the dialog if it is open. It is also responsible for state management.
  /// Should be called every ui update.
  pub fn show(&mut self, ctx: &Context) -> &Self {
    self.state = match self.state {
      State::Open => {
        if ctx.input(|state| state.key_pressed(Key::Escape)) {
          self.state = State::Cancelled;
        }

        let mut is_open = true;
        self.ui(ctx, &mut is_open);
        match is_open {
          true => self.state,
          false => State::Cancelled,
        }
      }
      _ => State::Closed,
    };

    self
  }

  fn ui(&mut self, ctx: &Context, is_open: &mut bool) {
    let mut window = Window::new(RichText::new(&self.title).strong())
      .open(is_open)
      .default_size(self.default_size)
      .resizable(self.resizable)
      .collapsible(false);

    if let Some(id) = self.id {
      window = window.id(id);
    }

    if let Some((align, offset)) = self.anchor {
      window = window.anchor(align, offset);
    }

    if let Some(current_pos) = self.current_pos {
      window = window.current_pos(current_pos);
    }

    window.show(ctx, |ui| self.ui_in_window(ui));
  }

  fn ui_in_window(&mut self, ui: &mut Ui) {
    enum Command {
      Cancel,
      CreateDirectory,
      Folder,
      Open(FileInfo),
      OpenSelected,
      BrowseDirectory(FileInfo),
      Refresh,
      Rename(PathBuf, PathBuf),
      Save(FileInfo),
      Select(FileInfo),
      MultiSelectRange(usize),
      MultiSelect(usize),
      MultiSelectSwitch(usize),
      UpDirectory,
    }
    let mut command: Option<Command> = None;

    // Top directory field with buttons.
    egui::TopBottomPanel::top("egui_file_top").show_inside(ui, |ui| {
      ui.horizontal(|ui| {
        ui.add_enabled_ui(self.path.parent().is_some(), |ui| {
          let response = ui.button("⬆").on_hover_text("Parent Folder");
          if response.clicked() {
            command = Some(Command::UpDirectory);
          }
        });
        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
          let response = ui.button("⟲").on_hover_text("Refresh");
          if response.clicked() {
            command = Some(Command::Refresh);
          }

          let response = ui.add_sized(
            ui.available_size(),
            TextEdit::singleline(&mut self.path_edit),
          );

          if response.lost_focus() {
            let path = PathBuf::from(&self.path_edit);
            command = Some(Command::Open(FileInfo::new(path)));
          }
        });
      });
      ui.add_space(ui.spacing().item_spacing.y);
    });

    // Bottom file field.
    egui::TopBottomPanel::bottom("egui_file_bottom").show_inside(ui, |ui| {
      ui.add_space(ui.spacing().item_spacing.y * 2.0);
      ui.horizontal(|ui| {
        ui.label("File:");
        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
          if self.new_folder && ui.button("New Folder").clicked() {
            command = Some(Command::CreateDirectory);
          }

          if self.rename {
            ui.add_enabled_ui(self.can_rename(), |ui| {
              if ui.button("Rename").clicked() {
                if let Some(from) = self.selected_file.clone() {
                  let to = from.path.with_file_name(&self.filename_edit);
                  command = Some(Command::Rename(from.path, to));
                }
              }
            });
          }

          let response = ui.add_sized(
            ui.available_size(),
            TextEdit::singleline(&mut self.filename_edit),
          );

          if response.lost_focus() {
            let ctx = response.ctx;
            let enter_pressed = ctx.input(|state| state.key_pressed(Key::Enter));

            if enter_pressed && (self.filename_filter)(self.filename_edit.as_str()) {
              let path = self.path.join(&self.filename_edit);
              match self.dialog_type {
                DialogType::SelectFolder => command = Some(Command::Folder),
                DialogType::OpenFile => {
                  if path.exists() {
                    command = Some(Command::Open(FileInfo::new(path)));
                  }
                }
                DialogType::SaveFile => {
                  command = Some(match path.is_dir() {
                    true => Command::Open(FileInfo {
                      path,
                      dir: true,
                      selected: false,
                    }),
                    false => Command::Save(FileInfo {
                      path,
                      dir: false,
                      selected: false,
                    }),
                  });
                }
              }
            }
          }
        });
      });

      ui.add_space(ui.spacing().item_spacing.y);

      // Confirm, Cancel buttons.
      ui.horizontal(|ui| {
        match self.dialog_type {
          DialogType::SelectFolder => {
            ui.horizontal(|ui| {
              if ui.button("Open").clicked() {
                command = Some(Command::Folder);
              };
            });
          }
          DialogType::OpenFile => {
            ui.horizontal(|ui| {
              ui.set_enabled(self.can_open());
              if ui.button("Open").clicked() {
                command = Some(Command::OpenSelected);
              };
            });
          }
          DialogType::SaveFile => {
            let should_open_directory = match &self.selected_file {
              Some(file) => file.dir,
              None => false,
            };

            if should_open_directory {
              if ui.button("Open").clicked() {
                command = Some(Command::OpenSelected);
              };
            } else {
              ui.horizontal(|ui| {
                ui.set_enabled(self.can_save());
                if ui.button("Save").clicked() {
                  let filename = &self.filename_edit;
                  let path = self.path.join(filename);
                  command = Some(Command::Save(FileInfo::new(path)));
                };
              });
            }
          }
        }

        if ui.button("Cancel").clicked() {
          command = Some(Command::Cancel);
        }

        #[cfg(unix)]
        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
          if ui.checkbox(&mut self.show_hidden, "Show Hidden").changed() {
            self.refresh();
          }
        });
      });
    });

    // File list.
    egui::CentralPanel::default().show_inside(ui, |ui| {
      ScrollArea::vertical().show_rows(
        ui,
        ui.text_style_height(&egui::TextStyle::Body),
        self.files.as_ref().map_or(0, |files| files.len()),
        |ui, range| match self.files.as_ref() {
          Ok(files) => {
            ui.with_layout(ui.layout().with_cross_justify(true), |ui| {
              let selected = self.selected_file.as_ref().map(|info| &info.path);
              let range_start = range.start;

              for (n, info) in files[range].iter().enumerate() {
                let idx = n + range_start;
                let label = match info.dir {
                  true => "🗀 ",
                  false => "🗋 ",
                }
                .to_string()
                  + get_file_name(info);

                let is_selected = if self.multi_select_enabled {
                  files[idx].selected
                } else {
                  Some(&info.path) == selected
                };
                let response = ui.selectable_label(is_selected, label);
                if response.clicked() {
                  if self.multi_select_enabled {
                    if ui.input(|i| i.modifiers.shift) {
                      command = Some(Command::MultiSelectRange(idx))
                    } else if ui.input(|i| i.modifiers.ctrl) {
                      command = Some(Command::MultiSelectSwitch(idx))
                    } else {
                      command = Some(Command::MultiSelect(idx))
                    }
                  } else {
                    command = Some(Command::Select(info.clone()));
                  }
                }

                if response.double_clicked() {
                  match self.dialog_type {
                    DialogType::SelectFolder => {
                      // Always open folder on double click, otherwise SelectFolder cant enter sub-folders.
                      command = Some(Command::OpenSelected);
                    }
                    // Open or save file only if name matches filter.
                    DialogType::OpenFile => {
                      if info.dir {
                        command = Some(Command::BrowseDirectory(info.clone()));
                      } else if (self.filename_filter)(self.filename_edit.as_str()) {
                        command = Some(Command::Open(info.clone()));
                      }
                    }
                    DialogType::SaveFile => {
                      if info.dir {
                        command = Some(Command::OpenSelected);
                      } else if (self.filename_filter)(self.filename_edit.as_str()) {
                        command = Some(Command::Save(info.clone()));
                      }
                    }
                  }
                }
              }
            })
            .response
          }
          Err(e) => ui.label(e.to_string()),
        },
      );
    });

    if let Some(command) = command {
      match command {
        Command::Select(info) => self.select(Some(info)),
        Command::MultiSelect(idx) => self.select_reset_multi(idx),
        Command::MultiSelectRange(idx) => self.select_range(idx),
        Command::MultiSelectSwitch(idx) => self.select_switch_multi(idx),
        Command::Folder => {
          let path = self.get_folder().to_owned();
          self.selected_file = Some(FileInfo {
            path,
            dir: true,
            selected: true,
          });
          self.confirm();
        }
        Command::Open(path) => {
          self.select(Some(path));
          self.open_selected();
        }
        Command::OpenSelected => self.open_selected(),
        Command::BrowseDirectory(dir) => {
          self.selected_file = Some(dir);
          self.open_selected();
        }
        Command::Save(file) => {
          self.selected_file = Some(file);
          self.confirm();
        }
        Command::Cancel => self.state = State::Cancelled,
        Command::Refresh => self.refresh(),
        Command::UpDirectory => {
          if self.path.pop() {
            self.refresh();
          }
        }
        Command::CreateDirectory => {
          let mut path = self.path.clone();
          let name = match self.filename_edit.is_empty() {
            true => "New folder",
            false => &self.filename_edit,
          };
          path.push(name);
          match fs::create_dir(&path) {
            Ok(_) => {
              self.refresh();
              self.select(Some(FileInfo::new(path)));
              // TODO: scroll to selected?
            }
            Err(err) => println!("Error while creating directory: {err}"),
          }
        }
        Command::Rename(from, to) => match fs::rename(from, &to) {
          Ok(_) => {
            self.refresh();
            self.select(Some(FileInfo::new(to)));
          }
          Err(err) => println!("Error while renaming: {err}"),
        },
      };
    }
  }

  fn get_folder(&self) -> &Path {
    if let Some(info) = &self.selected_file {
      if info.dir {
        return info.path.as_path();
      }
    }

    // No selected file or it's not a folder, so use the current path.
    &self.path
  }

  fn read_folder(&self) -> Result<Vec<FileInfo>, Error> {
    fs::read_dir(&self.path).map(|entries| {
      let mut file_infos: Vec<FileInfo> = entries
        .filter_map(|result| result.ok())
        .filter_map(|entry| {
          let info = FileInfo::new(entry.path());
          if !info.dir {
            // Do not show system files.
            if !info.path.is_file() {
              return None;
            }

            // Filter.
            if !(self.show_files_filter)(&info.path) {
              return None;
            }
          }

          #[cfg(unix)]
          if !self.show_hidden && get_file_name(&info).starts_with('.') {
            return None;
          }

          Some(info)
        })
        .collect();

      // Sort with folders before files.
      file_infos.sort_by(|a, b| match a.dir == b.dir {
        true => a.path.file_name().cmp(&b.path.file_name()),
        false => b.dir.cmp(&a.dir),
      });

      #[cfg(windows)]
      let file_infos = match self.show_drives {
        true => {
          let drives = get_drives();
          let mut infos = Vec::with_capacity(drives.len() + file_infos.len());
          for drive in drives {
            infos.push(FileInfo {
              path: drive,
              dir: true,
            });
          }
          infos.append(&mut file_infos);
          infos
        }
        false => file_infos,
      };

      file_infos
    })
  }
}

#[derive(Clone, Debug, Default)]
struct FileInfo {
  path: PathBuf,
  dir: bool,
  selected: bool,
}

impl FileInfo {
  fn new(path: PathBuf) -> Self {
    let dir = path.is_dir();
    Self {
      path,
      dir,
      selected: false,
    }
  }
}

#[cfg(windows)]
fn get_drives() -> Vec<PathBuf> {
  let mut drive_names = Vec::new();
  let mut drives = unsafe { GetLogicalDrives() };
  let mut letter = b'A';
  while drives > 0 {
    if drives & 1 != 0 {
      drive_names.push(format!("{}:\\", letter as char).into());
    }
    drives >>= 1;
    letter += 1;
  }
  drive_names
}

#[cfg(windows)]
fn is_drive_root(path: &Path) -> bool {
  path
    .to_str()
    .filter(|path| &path[1..] == ":\\")
    .and_then(|path| path.chars().next())
    .map_or(false, |ch| ch.is_ascii_uppercase())
}

fn get_file_name(info: &FileInfo) -> &str {
  #[cfg(windows)]
  if info.dir && is_drive_root(&info.path) {
    return info.path.to_str().unwrap_or_default();
  }
  info
    .path
    .file_name()
    .and_then(|name| name.to_str())
    .unwrap_or_default()
}

#[cfg(windows)]
extern "C" {
  pub fn GetLogicalDrives() -> u32;
}
