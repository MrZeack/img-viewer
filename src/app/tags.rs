//! Tags: asignación de etiquetas 1-9 a imágenes y movimiento a carpetas con su nombre.
use std::collections::{BTreeMap, HashMap};
use std::io;
use std::path::{Path, PathBuf};

pub(crate) const MAX_TAG_NAME_CHARS: usize = 64;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TagNameError {
    Empty,
    TooLong,
    InvalidChars,
    Reserved,
    Duplicate(u8),
}

pub(crate) fn validate_tag_name(raw: &str) -> Result<String, TagNameError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(TagNameError::Empty);
    }
    if name.chars().count() > MAX_TAG_NAME_CHARS {
        return Err(TagNameError::TooLong);
    }
    if name.starts_with('.') {
        return Err(TagNameError::Reserved);
    }
    let invalid = |c: char| {
        c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
    };
    if name.chars().any(invalid) || name.ends_with('.') {
        return Err(TagNameError::InvalidChars);
    }
    Ok(name.to_string())
}

impl std::fmt::Display for TagNameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "El nombre no puede estar vacío"),
            Self::TooLong => write!(f, "Máximo {MAX_TAG_NAME_CHARS} caracteres"),
            Self::InvalidChars => write!(f, "No uses / \\ : * ? \" < > | ni punto final"),
            Self::Reserved => write!(f, "No puede empezar con punto"),
            Self::Duplicate(slot) => write!(f, "Ya lo usa el tag {slot}"),
        }
    }
}

fn valid_slot(slot: u8) -> bool {
    (1..=9).contains(&slot)
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ToggleOutcome {
    Assigned(u8),
    Removed(u8),
    NeedsName(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlannedMove {
    pub(crate) src: PathBuf,
    pub(crate) dest: PathBuf,
    pub(crate) slot: u8,
}

#[derive(Debug, Default)]
pub(crate) struct MoveReport {
    pub(crate) moved: Vec<PlannedMove>,
    pub(crate) skipped: Vec<PathBuf>,
}

/// Modal abierto; mientras existe, los atajos globales quedan bloqueados.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TagModal {
    /// Pide el nombre de un tag; `target` recibe el tag al guardar.
    Name {
        slot: u8,
        target: Option<PathBuf>,
        input: String,
        error: Option<String>,
        focus_pending: bool,
    },
    /// Edita los nombres 1-9 (índice 0 = tag 1).
    Manage {
        inputs: Vec<String>,
        error: Option<String>,
    },
    /// Confirma el movimiento de las imágenes etiquetadas.
    ConfirmMove,
}

/// Nombres de tags (persistidos en config) y asignaciones de la sesión, por ruta.
#[derive(Debug, Default, Clone)]
pub(crate) struct TagStore {
    names: BTreeMap<u8, String>,
    assignments: HashMap<PathBuf, u8>,
}

impl TagStore {
    pub(crate) fn from_config(names: &BTreeMap<String, String>) -> Self {
        let mut store = Self::default();
        for (key, name) in names {
            if let Ok(slot) = key.parse::<u8>() {
                let _ = store.set_name(slot, name);
            }
        }
        store
    }

    pub(crate) fn to_config(&self) -> BTreeMap<String, String> {
        self.names
            .iter()
            .map(|(slot, name)| (slot.to_string(), name.clone()))
            .collect()
    }

    pub(crate) fn name(&self, slot: u8) -> Option<&str> {
        self.names.get(&slot).map(String::as_str)
    }

    pub(crate) fn set_name(&mut self, slot: u8, raw: &str) -> Result<(), TagNameError> {
        if !valid_slot(slot) {
            return Err(TagNameError::Reserved);
        }
        let name = validate_tag_name(raw)?;
        let lower = name.to_lowercase();
        if let Some((&other, _)) = self
            .names
            .iter()
            .find(|(&s, n)| s != slot && n.to_lowercase() == lower)
        {
            return Err(TagNameError::Duplicate(other));
        }
        self.names.insert(slot, name);
        Ok(())
    }

    /// Quita el nombre; falla si el tag aún tiene imágenes en `files`.
    pub(crate) fn clear_name(&mut self, slot: u8, files: &[PathBuf]) -> Result<(), String> {
        let in_use = self.counts(files).get(&slot).copied().unwrap_or(0);
        if in_use > 0 {
            return Err(format!(
                "El tag {slot} tiene {in_use} imagen(es); quítalas antes de borrar su nombre"
            ));
        }
        self.names.remove(&slot);
        Ok(())
    }

    pub(crate) fn toggle(&mut self, path: &Path, slot: u8) -> ToggleOutcome {
        if self.tag_of(path) == Some(slot) {
            self.assignments.remove(path);
            return ToggleOutcome::Removed(slot);
        }
        if self.name(slot).is_none() {
            return ToggleOutcome::NeedsName(slot);
        }
        self.assign(path, slot);
        ToggleOutcome::Assigned(slot)
    }

    pub(crate) fn assign(&mut self, path: &Path, slot: u8) {
        if valid_slot(slot) && self.names.contains_key(&slot) {
            self.assignments.insert(path.to_path_buf(), slot);
        }
    }

    pub(crate) fn untag(&mut self, path: &Path) -> Option<u8> {
        self.assignments.remove(path)
    }

    pub(crate) fn clear_assignments(&mut self) {
        self.assignments.clear();
    }

    pub(crate) fn tag_of(&self, path: &Path) -> Option<u8> {
        self.assignments.get(path).copied()
    }

    pub(crate) fn counts(&self, files: &[PathBuf]) -> BTreeMap<u8, usize> {
        let mut counts = BTreeMap::new();
        for slot in files.iter().filter_map(|f| self.tag_of(f)) {
            *counts.entry(slot).or_insert(0) += 1;
        }
        counts
    }

    pub(crate) fn plan_moves(&self, dir: &Path, files: &[PathBuf]) -> Vec<PlannedMove> {
        files
            .iter()
            .filter_map(|src| {
                let slot = self.tag_of(src)?;
                let name = self.name(slot)?;
                let file_name = src.file_name()?;
                Some(PlannedMove {
                    src: src.clone(),
                    dest: dir.join(name).join(file_name),
                    slot,
                })
            })
            .collect()
    }
}

/// Ejecuta el plan sin sobrescribir: si el destino existe, el archivo se omite.
pub(crate) fn execute_moves(
    plan: Vec<PlannedMove>,
    mover: impl Fn(&Path, &Path) -> io::Result<()>,
) -> MoveReport {
    let mut report = MoveReport::default();
    for planned in plan {
        if planned.dest.exists() || mover(&planned.src, &planned.dest).is_err() {
            report.skipped.push(planned.src);
        } else {
            report.moved.push(planned);
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    fn store_with(names: &[(u8, &str)]) -> TagStore {
        let mut store = TagStore::default();
        for (slot, name) in names {
            store.set_name(*slot, name).unwrap();
        }
        store
    }

    #[test]
    fn validate_tag_name_trims_and_accepts_unicode() {
        assert_eq!(
            validate_tag_name("  Viaje Japón  "),
            Ok("Viaje Japón".into())
        );
    }

    #[test]
    fn validate_tag_name_rejects_unsafe_names() {
        assert_eq!(validate_tag_name("   "), Err(TagNameError::Empty));
        assert_eq!(
            validate_tag_name(&"a".repeat(65)),
            Err(TagNameError::TooLong)
        );
        for bad in [
            "a/b", "a\\b", "a:b", "a*b", "a?b", "a\"b", "a<b", "a>b", "a|b", "a\nb", "fin.",
        ] {
            assert_eq!(
                validate_tag_name(bad),
                Err(TagNameError::InvalidChars),
                "{bad}"
            );
        }
        for bad in [".", "..", ".oculto"] {
            assert_eq!(validate_tag_name(bad), Err(TagNameError::Reserved), "{bad}");
        }
    }

    #[test]
    fn set_name_rejects_duplicates_case_insensitive_but_allows_same_slot() {
        let mut store = store_with(&[(1, "Familia")]);
        assert_eq!(
            store.set_name(2, "familia"),
            Err(TagNameError::Duplicate(1))
        );
        assert_eq!(store.set_name(1, "FAMILIA"), Ok(()));
        assert_eq!(store.name(1), Some("FAMILIA"));
    }

    #[test]
    fn toggle_requires_name_then_assigns_replaces_and_removes() {
        let mut store = store_with(&[(1, "a"), (2, "b")]);
        let img = p("/f/x.jpg");
        assert_eq!(store.toggle(&img, 3), ToggleOutcome::NeedsName(3));
        assert_eq!(store.tag_of(&img), None);
        assert_eq!(store.toggle(&img, 1), ToggleOutcome::Assigned(1));
        assert_eq!(store.toggle(&img, 2), ToggleOutcome::Assigned(2));
        assert_eq!(store.tag_of(&img), Some(2));
        assert_eq!(store.toggle(&img, 2), ToggleOutcome::Removed(2));
        assert_eq!(store.tag_of(&img), None);
    }

    #[test]
    fn untag_and_clear_assignments_keep_names() {
        let mut store = store_with(&[(1, "a")]);
        store.assign(&p("/f/x.jpg"), 1);
        store.assign(&p("/f/y.jpg"), 1);
        assert_eq!(store.untag(&p("/f/x.jpg")), Some(1));
        assert_eq!(store.untag(&p("/f/x.jpg")), None);
        store.clear_assignments();
        assert_eq!(store.tag_of(&p("/f/y.jpg")), None);
        assert_eq!(store.name(1), Some("a"));
    }

    #[test]
    fn counts_only_include_files_still_listed() {
        let mut store = store_with(&[(1, "a"), (2, "b")]);
        store.assign(&p("/f/x.jpg"), 1);
        store.assign(&p("/f/y.jpg"), 1);
        store.assign(&p("/f/z.jpg"), 2);
        let counts = store.counts(&[p("/f/x.jpg"), p("/f/z.jpg")]);
        assert_eq!(counts, BTreeMap::from([(1, 1), (2, 1)]));
    }

    #[test]
    fn plan_moves_targets_named_folder_in_listing_order() {
        let mut store = store_with(&[(1, "Familia"), (2, "Viaje")]);
        store.assign(&p("/f/b.jpg"), 2);
        store.assign(&p("/f/a.jpg"), 1);
        store.assign(&p("/f/gone.jpg"), 1);
        let files = [p("/f/a.jpg"), p("/f/b.jpg"), p("/f/c.jpg")];
        assert_eq!(
            store.plan_moves(Path::new("/f"), &files),
            vec![
                PlannedMove {
                    src: p("/f/a.jpg"),
                    dest: p("/f/Familia/a.jpg"),
                    slot: 1
                },
                PlannedMove {
                    src: p("/f/b.jpg"),
                    dest: p("/f/Viaje/b.jpg"),
                    slot: 2
                },
            ]
        );
    }

    #[test]
    fn clear_name_refuses_when_tag_in_use() {
        let mut store = store_with(&[(1, "a"), (2, "b")]);
        store.assign(&p("/f/x.jpg"), 1);
        assert!(store.clear_name(1, &[p("/f/x.jpg")]).is_err());
        assert_eq!(store.clear_name(2, &[p("/f/x.jpg")]), Ok(()));
        assert_eq!(store.name(2), None);
    }

    #[test]
    fn config_roundtrip_ignores_invalid_entries() {
        let raw = BTreeMap::from([
            ("1".to_string(), "Familia".to_string()),
            ("4".to_string(), "../escape".to_string()),
            ("10".to_string(), "fuera".to_string()),
            ("x".to_string(), "no".to_string()),
        ]);
        let store = TagStore::from_config(&raw);
        assert_eq!(store.name(1), Some("Familia"));
        assert_eq!(store.name(4), None);
        assert_eq!(
            store.to_config(),
            BTreeMap::from([("1".to_string(), "Familia".to_string())])
        );
    }

    #[test]
    fn execute_moves_moves_files_and_skips_existing_destinations() {
        let dir = std::env::temp_dir().join(format!("img-viewer-tags-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("Familia")).unwrap();
        fs::write(dir.join("a.jpg"), "a").unwrap();
        fs::write(dir.join("b.jpg"), "b").unwrap();
        fs::write(dir.join("Familia/b.jpg"), "existing").unwrap();

        let plan = vec![
            PlannedMove {
                src: dir.join("a.jpg"),
                dest: dir.join("Familia/a.jpg"),
                slot: 1,
            },
            PlannedMove {
                src: dir.join("b.jpg"),
                dest: dir.join("Familia/b.jpg"),
                slot: 1,
            },
        ];
        let report = execute_moves(plan, |src, dest| {
            fs::create_dir_all(dest.parent().unwrap())?;
            fs::rename(src, dest)
        });

        assert_eq!(report.moved.len(), 1);
        assert_eq!(report.skipped, vec![dir.join("b.jpg")]);
        assert!(dir.join("Familia/a.jpg").exists());
        assert!(dir.join("b.jpg").exists());
        assert_eq!(
            fs::read_to_string(dir.join("Familia/b.jpg")).unwrap(),
            "existing"
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
