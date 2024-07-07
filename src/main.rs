use aes::Aes256;
use block_modes::block_padding::Pkcs7;
use block_modes::{BlockMode, Cbc};
use eframe::epaint::FontFamily::{Monospace, Proportional};
use eframe::CreationContext;
use egui::{Color32, FontData, FontDefinitions, Image, Stroke, Vec2};
use hex::decode;
use rfd::FileDialog;
use rusqlite::{params, Connection};
use std::fs;
use std::io::{self, Write};

const FONT_INTER: &[u8] = include_bytes!("./Inter-Light.ttf");
const FONT_ROBOTO_MONO: &[u8] = include_bytes!("./RobotoMono-Light.ttf");

pub fn load_fonts() -> FontDefinitions {
    // Fonts

    let mut fonts = FontDefinitions::default();
    let font_inter = FontData::from_static(FONT_INTER);
    let roboto_mono = FontData::from_static(FONT_ROBOTO_MONO);

    fonts.font_data.insert("inter".into(), font_inter);
    fonts.font_data.insert("roboto_mono".into(), roboto_mono);

    fonts.families.insert(Monospace, vec!["roboto_mono".into()]);
    fonts.families.insert(Proportional, vec!["inter".into()]);

    fonts
}

type Aes256Cbc = Cbc<Aes256, Pkcs7>;

fn generate_iv() -> Vec<u8> {
    let mut iv = vec![0u8; 16];
    getrandom::getrandom(&mut iv).unwrap();
    iv
}

fn encrypt_file(input_filename: &str, output_filename: &str, key: &[u8]) -> io::Result<()> {
    let data = fs::read(input_filename)?;

    // let iv = generate_iv();
    let iv = "unique_initializ".as_bytes();
    let cipher = Aes256Cbc::new_from_slices(key, &iv).unwrap();
    let ciphertext = cipher.encrypt_vec(&data);

    let mut file = fs::File::create(output_filename)?;
    file.write_all(&iv)?;
    file.write_all(&ciphertext)?;

    Ok(())
}

fn decrypt_file(input_filename: &str, output_filename: &str, key: &[u8]) -> io::Result<()> {
    let data = fs::read(input_filename)?;
    let (iv, ciphertext) = data.split_at(16);

    let cipher = Aes256Cbc::new_from_slices(key, iv).unwrap();
    let decrypted_data = cipher.decrypt_vec(ciphertext).unwrap();

    let mut file = fs::File::create(output_filename)?;
    file.write_all(&decrypted_data)?;

    Ok(())
}

fn create_db(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute(
        "CREATE TABLE IF NOT EXISTS notes (
            id INTEGER PRIMARY KEY,
            title TEXT NOT NULL,
            content TEXT NOT NULL,
            hidden BOOLEAN NOT NULL DEFAULT 0
        )",
        params![],
    )?;
    Ok(())
}

fn add_note(connection: &Connection, title: &str, content: &str) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO notes (title, content, hidden) VALUES (?1, ?2, 0)",
        params![title, content],
    )?;
    Ok(())
}

fn get_notes(connection: &Connection) -> rusqlite::Result<Vec<(i32, String)>> {
    let mut stmt = connection.prepare("SELECT id, title FROM notes WHERE hidden != 1")?;
    let note_iter = stmt.query_map(params![], |row| Ok((row.get(0)?, row.get::<_, String>(1)?)))?;

    let mut notes = Vec::new();
    for note in note_iter {
        notes.push(note?);
    }
    Ok(notes)
}

fn get_note_content(connection: &Connection, note_id: i32) -> rusqlite::Result<(String, String)> {
    let mut stmt = connection.prepare("SELECT title, content FROM notes WHERE id = ?1")?;
    let note = stmt.query_row(params![note_id], |row| {
        Ok((row.get(0)?, row.get::<_, String>(1)?))
    })?;
    Ok(note)
}

fn update_note(
    connection: &Connection,
    note_id: i32,
    title: &str,
    content: &str,
) -> rusqlite::Result<()> {
    connection.execute(
        "UPDATE notes SET title = ?1, content = ?2 WHERE id = ?3",
        params![title, content, note_id],
    )?;
    Ok(())
}

fn hide_note(connection: &Connection, note_id: i32) -> rusqlite::Result<()> {
    connection.execute(
        "UPDATE notes SET hidden = 1 WHERE id = ?1",
        params![note_id],
    )?;
    Ok(())
}

#[derive(PartialEq)]
enum AppState {
    Initial,
    Notes,
}

struct MyApp {
    state: AppState,
    db_file: String,
    key: String,
    notes: Vec<(i32, String)>,
    selected_note: Option<i32>,
    note_title: String,
    note_content: String,
    new_note_title: String,
    new_note_content: String,
    status: String,
}

impl MyApp {
    fn default(cc: &CreationContext) -> Self {
        cc.egui_ctx.set_fonts(load_fonts());

        Self {
            state: AppState::Initial,
            db_file: String::new(),
            key: String::new(),
            notes: Vec::new(),
            selected_note: None,
            note_title: String::new(),
            note_content: String::new(),
            new_note_title: String::new(),
            new_note_content: String::new(),
            status: String::new(),
        }
    }
}

impl MyApp {
    fn load_notes(&mut self) {
        let temp_db_file = "temp_db.sqlite";
        let key = decode(&self.key).expect("Invalid key format");
        if decrypt_file(&self.db_file, temp_db_file, &key).is_ok() {
            let connection = Connection::open(temp_db_file).expect("Failed to open DB");
            match get_notes(&connection) {
                Ok(notes) => {
                    self.notes = notes;
                    self.status = "Database opened successfully".to_string();
                }
                Err(err) => {
                    self.status = format!("Failed to get notes: {}", err);
                }
            }
            connection.close().expect("Failed to close DB");
            fs::remove_file(temp_db_file).expect("Failed to remove temp file");
            self.state = AppState::Notes;
        } else {
            self.status = "Failed to open database".to_string();
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        match self.state {
            AppState::Initial => {
                self.show_initial_screen(ctx);
            }
            AppState::Notes => {
                self.show_notes_screen(ctx);
            }
        }
    }
}

impl MyApp {
    fn show_initial_screen(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(50.0); // Отступ сверху в пикселях
            ui.vertical_centered(|ui| {
                // Image
                ui.add(
                    Image::new(egui::include_image!("unknown_logo.webp")).fit_to_original_size(0.7),
                );
                // Image
                ui.add_space(50.0);

                ui.style_mut().text_styles.insert(
                    egui::TextStyle::Button,
                    egui::FontId::new(16.0, Proportional),
                );
                ui.style_mut().text_styles.insert(
                    egui::TextStyle::Heading,
                    egui::FontId::new(16.0, Proportional),
                );

                ui.heading("Select or create Vauilt");
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.style_mut().visuals.widgets.inactive.weak_bg_fill =
                        Color32::from_rgb(0, 128, 128);
                    ui.style_mut().visuals.widgets.hovered.weak_bg_fill =
                        Color32::from_rgb(0, 90, 90);
                    ui.style_mut().visuals.widgets.hovered.bg_stroke = Stroke::NONE;
                    ui.style_mut().spacing.button_padding = Vec2::new(10.0, 8.0);

                    ui.add_space(ui.available_width() * 0.5 - 130.0); // Центральный отступ
                    if ui.button("Select Vault").clicked() {
                        if let Some(path) = FileDialog::new().pick_file() {
                            self.db_file = path.display().to_string();
                        }
                    }
                    if ui.button("Create new Vault").clicked() {
                        if let Some(path) = FileDialog::new().save_file() {
                            self.db_file = path.display().to_string();
                            let key = decode(&self.key).expect("Invalid key format");
                            let temp_db_file = "temp_db.sqlite";
                            let connection =
                                Connection::open(temp_db_file).expect("Failed to create DB");
                            create_db(&connection).expect("Failed to create tables");
                            connection.close().expect("Failed to close DB");
                            encrypt_file(temp_db_file, &self.db_file, &key)
                                .expect("Failed to encrypt DB");
                            fs::remove_file(temp_db_file).expect("Failed to remove temp file");
                            self.status =
                                "New database created and encrypted successfully".to_string();
                        }
                    }
                    ui.label(&self.db_file);
                });

                ui.label("Key (hex):");
                ui.style_mut().visuals.widgets.hovered.bg_stroke = Stroke::NONE;

                ui.text_edit_singleline(&mut self.key);

                if ui.button("Open Vauit").clicked() {
                    self.load_notes();
                }

                ui.label(&self.status);
            });
        });
    }

    fn show_notes_screen(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if ui.button("Back").clicked() {
                self.state = AppState::Initial;
                self.db_file.clear();
                self.key.clear();
                self.notes.clear();
                self.selected_note = None;
                self.note_title.clear();
                self.note_content.clear();
                self.new_note_title.clear();
                self.new_note_content.clear();
                self.status.clear();
            }

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label("Notes:");
                    for (id, title) in &self.notes {
                        if ui
                            .selectable_label(self.selected_note == Some(*id), title)
                            .clicked()
                        {
                            self.selected_note = Some(*id);
                            let temp_db_file = "temp_db.sqlite";
                            let key = decode(&self.key).expect("Invalid key format");
                            if decrypt_file(&self.db_file, temp_db_file, &key).is_ok() {
                                let connection =
                                    Connection::open(temp_db_file).expect("Failed to open DB");
                                match get_note_content(&connection, *id) {
                                    Ok((title, content)) => {
                                        self.note_title = title;
                                        self.note_content = content;
                                    }
                                    Err(err) => {
                                        self.status = format!("Failed to get note content: {}", err)
                                    }
                                }
                                connection.close().expect("Failed to close DB");
                                fs::remove_file(temp_db_file).expect("Failed to remove temp file");
                            }
                        }
                    }
                    if ui.button("New Note").clicked() {
                        self.new_note_title.clear();
                        self.new_note_content.clear();
                        self.selected_note = None;
                    }
                });

                ui.vertical(|ui| {
                    if let Some(note_id) = self.selected_note {
                        ui.label("Edit Note:");
                        ui.text_edit_singleline(&mut self.note_title);
                        ui.text_edit_multiline(&mut self.note_content);

                        if ui.button("Save Changes").clicked() {
                            let temp_db_file = "temp_db.sqlite";
                            let key = decode(&self.key).expect("Invalid key format");
                            if decrypt_file(&self.db_file, temp_db_file, &key).is_ok() {
                                let connection =
                                    Connection::open(temp_db_file).expect("Failed to open DB");
                                match update_note(
                                    &connection,
                                    note_id,
                                    &self.note_title,
                                    &self.note_content,
                                ) {
                                    Ok(_) => {
                                        self.notes =
                                            get_notes(&connection).expect("Failed to get notes");
                                        self.status = "Note updated successfully".to_string();
                                    }
                                    Err(err) => {
                                        self.status = format!("Failed to update note: {}", err);
                                    }
                                }
                                connection.close().expect("Failed to close DB");
                                encrypt_file(temp_db_file, &self.db_file, &key)
                                    .expect("Failed to encrypt DB");
                                fs::remove_file(temp_db_file).expect("Failed to remove temp file");
                            }
                        }

                        if ui.button("Delete Note").clicked() {
                            let temp_db_file = "temp_db.sqlite";
                            let key = decode(&self.key).expect("Invalid key format");
                            if decrypt_file(&self.db_file, temp_db_file, &key).is_ok() {
                                let connection =
                                    Connection::open(temp_db_file).expect("Failed to open DB");
                                match hide_note(&connection, note_id) {
                                    Ok(_) => {
                                        self.notes =
                                            get_notes(&connection).expect("Failed to get notes");
                                        self.selected_note = None;
                                        self.note_title.clear();
                                        self.note_content.clear();
                                        self.status = "Note hidden successfully".to_string();
                                    }
                                    Err(err) => {
                                        self.status = format!("Failed to hide note: {}", err);
                                    }
                                }
                                connection.close().expect("Failed to close DB");
                                encrypt_file(temp_db_file, &self.db_file, &key)
                                    .expect("Failed to encrypt DB");
                                fs::remove_file(temp_db_file).expect("Failed to remove temp file");
                            }
                        }
                    } else {
                        ui.label("New Note:");
                        ui.text_edit_singleline(&mut self.new_note_title);
                        ui.text_edit_multiline(&mut self.new_note_content);

                        if ui.button("Add Note").clicked() {
                            let temp_db_file = "temp_db.sqlite";
                            let key = decode(&self.key).expect("Invalid key format");
                            if decrypt_file(&self.db_file, temp_db_file, &key).is_ok() {
                                let connection =
                                    Connection::open(temp_db_file).expect("Failed to open DB");
                                match add_note(
                                    &connection,
                                    &self.new_note_title,
                                    &self.new_note_content,
                                ) {
                                    Ok(_) => {
                                        self.notes =
                                            get_notes(&connection).expect("Failed to get notes");
                                        self.status = "Note added successfully".to_string();
                                    }
                                    Err(err) => {
                                        self.status = format!("Failed to add note: {}", err);
                                    }
                                }
                                connection.close().expect("Failed to close DB");
                                encrypt_file(temp_db_file, &self.db_file, &key)
                                    .expect("Failed to encrypt DB");
                                fs::remove_file(temp_db_file).expect("Failed to remove temp file");
                            }
                        }
                    }
                });
            });

            ui.label(&self.status);
        });
    }
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_min_inner_size([500.0, 700.0])
            .with_max_inner_size([500.0, 700.0]),
        ..Default::default()
    };
    eframe::run_native(
        "sefy",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(MyApp::default(cc)))
        }),
    )
}
