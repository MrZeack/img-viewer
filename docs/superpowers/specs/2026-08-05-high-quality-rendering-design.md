# High Quality (HQ) / Full Resolution On-Demand Rendering Design Specification

## Goal
Permitir al usuario inspeccionar el máximo detalle en la imagen activa mediante la tecla **`H`**, alternando la calidad de renderizado en 3 niveles (*1x Normal*, *2x/3x HQ*, *Max/Original*) sin ralentizar la navegación general ni agotar la memoria RAM.

## Design Details

### 1. Estado de la Aplicación (`src/app/state.rs`)
- Agregar `pub(crate) quality_multiplier: u8` a la estructura `ImageViewerApp`:
  - `1`: Normal (`max_cached_dimension`)
  - `2`: Alta calidad (multiplicador x2 en dimensión)
  - `3`: Calidad nativa completa (`None` en dimensión máxima / resolución original completa).

### 2. Atajo de Teclado y Comportamiento (`src/app/input.rs` & `src/app/core.rs`)
- En `handle_keyboard_input` (`src/app/input.rs`):
  - Al presionar **`H`**:
    - `self.quality_multiplier = (self.quality_multiplier % 3) + 1` (ciclo 1 -> 2 -> 3 -> 1).
    - Llama a `self.load_current_image_with_quality(ctx, false)` para recargar la imagen actual manteniendo el zoom y la posición.
    - Muestra mensaje en la barra de estado:
      - `1` -> `"Calidad: Normal"`
      - `2` -> `"Calidad: Alta (2x)"`
      - `3` -> `"Calidad: Original (Resolución Máxima)"`
- **Reset al Navegar**:
  - Al cambiar de imagen (`next_image`, `previous_image`), `quality_multiplier` se resetea automáticamente a `1` (Normal) para no saturar la memoria RAM.

### 3. Carga con Resolución Personalizada (`src/app/core.rs`)
- Implementar `load_current_image_with_quality(&mut self, ctx: &Context, reset_view: bool)`:
  - Si `quality_multiplier == 1`: dimensión `(max_cached_dimension, max_cached_dimension)`.
  - Si `quality_multiplier == 2`: dimensión `(max_cached_dimension * 2, max_cached_dimension * 2)`.
  - Si `quality_multiplier == 3`: dimensión `None` (cargando la resolución nativa original completa).
  - Emite la solicitud a `image_loader.load_image`.

### 4. Feedback en Interfaz Grafica (`src/app/ui.rs`)
- En la barra de estado (`status_bar`), mostrar ` - Calidad: 2x` o ` - Calidad: MAX` si es `> 1`.
- En la leyenda de ayuda inferior, incluir `H: Calidad`.
