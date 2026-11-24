use eframe::egui;
use std::path::PathBuf;

#[path = "app.rs"]
mod app;
mod cache;
mod composition;
mod config;
mod cr3;
mod errors;
mod event;
mod focus;
mod image_loader;
mod metadata_loader;
mod metadata_simple;
mod scoring;
mod thumbnails;

use app::ImageViewerApp;

#[derive(Debug, PartialEq, Eq)]
enum LaunchAction {
    PrintVersion,
    PrintHelp,
    Open {
        dir: PathBuf,
        select_file: Option<PathBuf>,
    },
    Error(String),
}

fn resolve_launch_target(args: &[String], fallback_dir: PathBuf) -> LaunchAction {
    if args.iter().any(|a| a == "--version" || a == "-V") {
        return LaunchAction::PrintVersion;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        return LaunchAction::PrintHelp;
    }

    let mut path_arg: Option<&String> = None;
    for arg in args {
        if arg.starts_with('-') {
            return LaunchAction::Error(format!(
                "Opción desconocida: {arg}. Usa --help para ver el uso."
            ));
        }
        if path_arg.is_some() {
            return LaunchAction::Error(
                "Solo se admite una ruta de carpeta o archivo. Usa --help para ver el uso.".into(),
            );
        }
        path_arg = Some(arg);
    }

    let Some(raw) = path_arg else {
        return LaunchAction::Open {
            dir: fallback_dir,
            select_file: None,
        };
    };

    let target = PathBuf::from(raw);
    let abs = if target.is_absolute() {
        target
    } else {
        fallback_dir.join(target)
    };
    if !abs.exists() {
        return LaunchAction::Error(format!("La ruta no existe: {}", abs.display()));
    }
    if abs.is_dir() {
        LaunchAction::Open {
            dir: abs,
            select_file: None,
        }
    } else if let Some(parent) = abs.parent() {
        LaunchAction::Open {
            dir: parent.to_path_buf(),
            select_file: Some(abs),
        }
    } else {
        LaunchAction::Error(format!(
            "No se pudo determinar la carpeta de: {}",
            abs.display()
        ))
    }
}

fn main() -> Result<(), eframe::Error> {
    let version = env!("CARGO_PKG_VERSION");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let fallback_dir = std::env::current_dir().unwrap_or_else(|err| {
        eprintln!("Warning: Could not get current directory: {}", err);
        PathBuf::from(".")
    });

    let (initial_dir, initial_select) = match resolve_launch_target(&args, fallback_dir) {
        LaunchAction::PrintVersion => {
            println!("img-viewer {version}");
            return Ok(());
        }
        LaunchAction::PrintHelp => {
            println!(
                "img-viewer {version}\n\nUso:\n  img [CARPETA | IMAGEN]\n\nOpciones:\n  -h, --help       Muestra esta ayuda\n  -V, --version    Muestra la versión"
            );
            return Ok(());
        }
        LaunchAction::Error(msg) => {
            eprintln!("Error: {msg}");
            std::process::exit(2);
        }
        LaunchAction::Open { dir, select_file } => (dir, select_file),
    };

    let config = config::load_config();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_title(format!("Image Viewer v{}", version))
            .with_maximize_button(true),
        ..Default::default()
    };

    eframe::run_native(
        &format!("Image Viewer v{}", version),
        options,
        Box::new(move |cc| {
            let mut style = (*cc.egui_ctx.style()).clone();
            style.visuals = egui::Visuals::dark();
            cc.egui_ctx.set_style(style);

            Ok(Box::new(ImageViewerApp::new(
                cc,
                config,
                initial_dir,
                initial_select,
            )))
        }),
    )
}

#[cfg(test)]
mod cli_unit_tests {
    use super::*;

    #[test]
    fn resolve_launch_target_handles_flags_dirs_files_and_errors() {
        let cwd = std::env::temp_dir().join(format!("img-cli-unit-{}", std::process::id()));
        let sub = cwd.join("photos");
        std::fs::create_dir_all(&sub).unwrap();
        let img = sub.join("a.jpg");
        std::fs::write(&img, b"").unwrap();

        assert_eq!(
            resolve_launch_target(&["--version".into()], cwd.clone()),
            LaunchAction::PrintVersion
        );
        assert_eq!(
            resolve_launch_target(&["-h".into()], cwd.clone()),
            LaunchAction::PrintHelp
        );
        assert_eq!(
            resolve_launch_target(&[], cwd.clone()),
            LaunchAction::Open {
                dir: cwd.clone(),
                select_file: None
            }
        );
        assert_eq!(
            resolve_launch_target(&["photos".into()], cwd.clone()),
            LaunchAction::Open {
                dir: sub.clone(),
                select_file: None
            }
        );
        assert_eq!(
            resolve_launch_target(&[img.to_string_lossy().to_string()], cwd.clone()),
            LaunchAction::Open {
                dir: sub.clone(),
                select_file: Some(img)
            }
        );
        assert!(matches!(
            resolve_launch_target(&["--bogus".into()], cwd.clone()),
            LaunchAction::Error(_)
        ));
        assert!(matches!(
            resolve_launch_target(&["does_not_exist".into()], cwd.clone()),
            LaunchAction::Error(_)
        ));

        std::fs::remove_dir_all(&cwd).unwrap();
    }
}
