//! Modal de ayuda con los atajos de teclado (F1 o ?), en lugar de la barra de estado.
use super::state::ImageViewerApp;
use eframe::egui::{self, Align2, Context, Key};

pub(crate) struct Section {
    pub(crate) title: &'static str,
    pub(crate) items: &'static [(&'static str, &'static str)],
}

pub(crate) const SECTIONS: &[Section] = &[
    Section {
        title: "General",
        items: &[
            ("F1 / ?", "Mostrar u ocultar esta ayuda"),
            ("G", "Cuadrícula / vista individual"),
            ("S", "Cambiar orden: fecha ↑, fecha ↓, nota ↓, nota ↑"),
            ("Esc", "Salir (en la cuadrícula: volver a la imagen)"),
        ],
    },
    Section {
        title: "Vista individual",
        items: &[
            ("← →", "Imagen anterior / siguiente"),
            ("Rueda / pellizco", "Zoom"),
            ("+ / - / Espacio", "Acercar / alejar / ajustar a ventana"),
            ("Z / Doble clic", "Alternar zoom 100% (1:1) / ajustar"),
            ("Arrastrar", "Mover la imagen con zoom"),
            ("I / Tab", "Mostrar u ocultar panel lateral"),
            ("R / Shift+R", "Rotar a la derecha / izquierda"),
            (
                "C / Shift+C",
                "Composición: activar o cambiar guía / desactivar",
            ),
            ("↑ ↓", "Guía de composición siguiente / anterior"),
            ("F", "Focus peaking"),
            ("H", "Calidad: normal, alta (3x), original"),
        ],
    },
    Section {
        title: "Cuadrícula",
        items: &[
            ("Flechas", "Mover la selección"),
            (
                "RePág / AvPág / Inicio / Fin",
                "Saltar por páginas o a los extremos",
            ),
            ("Enter / Doble clic", "Abrir la imagen seleccionada"),
            ("+ / - / pellizco", "Tamaño de las miniaturas"),
        ],
    },
    Section {
        title: "Clasificar",
        items: &[
            ("L", "Mover a best/"),
            ("W", "Mover a worse/"),
            ("D", "Mover a deleted/"),
            ("U", "Deshacer el último movimiento"),
        ],
    },
    Section {
        title: "Tags",
        items: &[
            (
                "1-9",
                "Asignar o quitar el tag (la primera vez pide el nombre)",
            ),
            ("0", "Quitar el tag de la imagen"),
            ("T", "Editar nombres de tags"),
            ("M", "Mover las imágenes con tag a su carpeta"),
        ],
    },
];

impl ImageViewerApp {
    pub(crate) fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    /// Con la ayuda abierta consume el teclado: F1, ? o Esc la cierran (Esc no cierra la app).
    /// Devuelve `true` si la ayuda estaba abierta o se abrió en este frame.
    pub(crate) fn handle_help_keys(&mut self, ctx: &Context) -> bool {
        let toggle = ctx.input(|i| i.key_pressed(Key::F1) || i.key_pressed(Key::Questionmark));
        if self.show_help {
            if toggle || ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.show_help = false;
            }
            return true;
        }
        if toggle {
            self.show_help = true;
        }
        toggle
    }

    pub(crate) fn show_help_modal(&mut self, ctx: &Context) {
        if !self.show_help {
            return;
        }
        let mut open = true;
        egui::Window::new("Atajos de teclado")
            .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.columns(2, |cols| {
                    for (i, section) in SECTIONS.iter().enumerate() {
                        let ui = &mut cols[i % 2];
                        ui.strong(section.title);
                        egui::Grid::new(section.title).striped(true).show(ui, |ui| {
                            for (keys, action) in section.items {
                                ui.monospace(*keys);
                                ui.label(*action);
                                ui.end_row();
                            }
                        });
                        ui.add_space(8.0);
                    }
                });
                ui.separator();
                ui.weak("F1, ? o Esc para cerrar");
            });
        if !open {
            self.show_help = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Event, RawInput};

    fn press(app: &mut ImageViewerApp, key: Key) {
        let ctx = Context::default();
        let input = RawInput {
            events: vec![Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| app.handle_keyboard_input(ctx));
    }

    #[test]
    fn every_handled_shortcut_is_documented_once_per_section() {
        let all: Vec<&str> = SECTIONS
            .iter()
            .flat_map(|s| s.items.iter().map(|(k, _)| *k))
            .collect();
        for key in [
            "F1 / ?",
            "G",
            "S",
            "Esc",
            "← →",
            "R / Shift+R",
            "C / Shift+C",
            "↑ ↓",
            "F",
            "H",
            "I / Tab",
            "Z / Doble clic",
            "+ / - / Espacio",
            "L",
            "W",
            "D",
            "U",
            "1-9",
            "0",
            "T",
            "M",
        ] {
            assert!(all.contains(&key), "falta {key}");
        }
        for section in SECTIONS {
            let mut keys: Vec<_> = section.items.iter().map(|(k, _)| k).collect();
            keys.sort();
            keys.dedup();
            assert_eq!(
                keys.len(),
                section.items.len(),
                "repetida en {}",
                section.title
            );
        }
    }

    #[test]
    fn f1_opens_and_escape_closes_help_without_quitting() {
        let mut app = super::super::state::test_app(vec![], 0);
        press(&mut app, Key::F1);
        assert!(app.show_help);
        press(&mut app, Key::Escape);
        assert!(!app.show_help);
        assert!(
            !app.should_close,
            "Esc con la ayuda abierta no cierra la app"
        );
    }

    #[test]
    fn help_blocks_other_shortcuts_and_question_mark_toggles() {
        let mut app = super::super::state::test_app(vec![], 0);
        press(&mut app, Key::Questionmark);
        assert!(app.show_help);
        press(&mut app, Key::G);
        assert!(!app.show_grid, "G no actúa con la ayuda abierta");
        press(&mut app, Key::Questionmark);
        assert!(!app.show_help);
    }
}
