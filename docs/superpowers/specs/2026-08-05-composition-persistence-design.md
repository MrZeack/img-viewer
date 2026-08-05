# Composition Interaction & Persistence Design Specification

## Goal
Rediseñar el flujo de control del teclado para la guía de composición (`C` para activar/avanzar tipo, `Shift+C` para desactivar) y guardar persistentemente el estado de **Composición** (activa y tipo) y de **Focus Peaking** en el archivo `config.toml`.

## Design Details

### 1. Extensión de Configuración (`src/config.rs`)
- Agregar `ViewConfig` a `Config`:
  ```rust
  #[derive(Deserialize, Serialize, Debug, Clone)]
  pub struct ViewConfig {
      pub show_composition: bool,
      pub composition_type: String,
      pub show_focus_peaking: bool,
  }
  ```
- Configuración por defecto:
  ```rust
  view: ViewConfig {
      show_composition: false,
      composition_type: "RuleOfThirds".to_string(),
      show_focus_peaking: false,
  }
  ```
- Implementar función `pub fn save_config(config: &Config)` para persistir los cambios en `config.toml`.

### 2. Atajos de Teclado y Flujo de Composición (`src/app/input.rs`)
- **Tecla `C`**:
  - Si `!self.show_composition`:
    - `self.show_composition = true;`
    - Muestra mensaje `"Composición activada: [Nombre]"`.
  - Si `self.show_composition`:
    - `self.composition_type = self.composition_type.next();`
    - `self.composition_cache.clear();`
    - Muestra mensaje `"Composición: [Nombre]"`.
  - Guarda la configuración actualizada llamando a `save_config`.
- **Tecla `Shift + C`**:
  - `self.show_composition = false;`
  - Muestra mensaje `"Composición desactivada"`.
  - Guarda la configuración actualizada.
- **Tecla `F` (Focus Peaking)**:
  - Al alternar `self.show_focus_peaking`, guarda la configuración actualizada.

### 3. Carga Inicial (`src/app/core.rs`)
- En `ImageViewerApp::new`, inicializar `show_composition`, `composition_type` y `show_focus_peaking` desde `config.view`.
