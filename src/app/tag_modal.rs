//! Modales de tags: nombrar un tag, editar nombres y confirmar el movimiento.
use super::state::ImageViewerApp;
use super::tags::{TagModal, MAX_TAG_NAME_CHARS};
use eframe::egui::{self, Align2, Color32, Context, Key, RichText, TextEdit};

enum Action {
    None,
    Cancel,
    SubmitName(String),
    SubmitManager(Vec<String>),
    Move,
}

impl ImageViewerApp {
    pub(crate) fn show_tag_modal(&mut self, ctx: &Context) {
        let Some(mut modal) = self.tag_modal.take() else {
            return;
        };
        let enter = ctx.input(|i| i.key_pressed(Key::Enter));
        let escape = ctx.input(|i| i.key_pressed(Key::Escape));
        let mut action = if escape { Action::Cancel } else { Action::None };

        let title = match &modal {
            TagModal::Name { slot, .. } => format!("Nombre del tag {slot}"),
            TagModal::Manage { .. } => "Tags".to_string(),
            TagModal::ConfirmMove => "Mover imágenes con tag".to_string(),
        };

        egui::Window::new(title)
            .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| match &mut modal {
                TagModal::Name {
                    input,
                    error,
                    focus_pending,
                    ..
                } => {
                    ui.label("Se usará como nombre de la carpeta destino.");
                    let response = ui.add(
                        TextEdit::singleline(input)
                            .char_limit(MAX_TAG_NAME_CHARS)
                            .hint_text("p. ej. Familia")
                            .desired_width(260.0),
                    );
                    // El primer frame no toma foco: la tecla que abrió el modal no se escribe.
                    if *focus_pending {
                        *focus_pending = false;
                        ctx.request_repaint();
                    } else if !response.has_focus() {
                        response.request_focus();
                    }
                    show_error(ui, error.as_deref());
                    ui.horizontal(|ui| {
                        if ui.button("Guardar (Enter)").clicked() || enter {
                            action = Action::SubmitName(input.clone());
                        }
                        if ui.button("Cancelar (Esc)").clicked() {
                            action = Action::Cancel;
                        }
                    });
                }
                TagModal::Manage { inputs, error } => {
                    let counts = self.tags.counts(&self.image_files);
                    ui.label("Vacío = sin nombre. Los nombres se guardan en config.toml.");
                    egui::Grid::new("tag_manager_grid").show(ui, |ui| {
                        for (i, input) in inputs.iter_mut().enumerate() {
                            let slot = i as u8 + 1;
                            ui.label(format!("{slot}"));
                            ui.add(
                                TextEdit::singleline(input)
                                    .char_limit(MAX_TAG_NAME_CHARS)
                                    .desired_width(220.0),
                            );
                            ui.label(format!("{} img", counts.get(&slot).unwrap_or(&0)));
                            ui.end_row();
                        }
                    });
                    show_error(ui, error.as_deref());
                    ui.horizontal(|ui| {
                        if ui.button("Guardar (Enter)").clicked() || enter {
                            action = Action::SubmitManager(inputs.clone());
                        }
                        if ui.button("Cancelar (Esc)").clicked() {
                            action = Action::Cancel;
                        }
                    });
                }
                TagModal::ConfirmMove => {
                    let counts = self.tags.counts(&self.image_files);
                    ui.label(format!("Destino: {}", self.current_dir.display()));
                    for (slot, count) in &counts {
                        let name = self.tags.name(*slot).unwrap_or_default();
                        ui.label(format!("{slot} · {name}/ ← {count} imagen(es)"));
                    }
                    ui.label(
                        RichText::new(
                            "Los archivos que ya existan en destino se omiten. U deshace.",
                        )
                        .weak(),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Mover (Enter)").clicked() || enter {
                            action = Action::Move;
                        }
                        if ui.button("Cancelar (Esc)").clicked() {
                            action = Action::Cancel;
                        }
                    });
                }
            });

        match action {
            Action::None => self.tag_modal = Some(modal),
            Action::Cancel => {}
            Action::SubmitName(input) => {
                self.tag_modal = Some(modal);
                self.submit_tag_name(&input);
            }
            Action::SubmitManager(inputs) => {
                if let Err(e) = self.submit_tag_manager(&inputs) {
                    if let TagModal::Manage { error, .. } = &mut modal {
                        *error = Some(e);
                    }
                    self.tag_modal = Some(modal);
                }
            }
            Action::Move => self.move_tagged(ctx),
        }
    }
}

fn show_error(ui: &mut egui::Ui, error: Option<&str>) {
    if let Some(error) = error {
        ui.label(RichText::new(error).color(Color32::from_rgb(230, 90, 90)));
    }
}
