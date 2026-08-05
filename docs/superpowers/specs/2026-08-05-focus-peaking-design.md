# Enhanced Focus Peaking (Laplacian of Gaussian - LoG) Design Specification

## Goal
Mejorar la precisión del **Focus Peaking** en `img-viewer` reemplazando el operador Sobel por un algoritmo de **Laplaciano sobre suavizado Gaussiano (LoG)** para eliminar falsos positivos en zonas desenfocadas o con ruido ISO.

## Design Details

### 1. Algoritmo Laplaciano de Gaussiana (`src/focus.rs`)
- **Paso 1: Suavizado Gaussiano (Filtro Paso Bajo 3x3)**
  - Para reducir el ruido digital de alta frecuencia que causa falsos positivos en áreas desenfocadas, se aplica un filtro kernel Gaussiano 3x3:
    $$\text{Kernel Gaussiano} = \frac{1}{16} \begin{bmatrix} 1 & 2 & 1 \\ 2 & 4 & 2 \\ 1 & 2 & 1 \end{bmatrix}$$
- **Paso 2: Operador Laplaciano (Filtro Paso Alto 3x3)**
  - Sobre la imagen suavizada, se aplica el filtro Laplaciano estándar de 2º orden:
    $$\text{Kernel Laplaciano} = \begin{bmatrix} 0 & 1 & 0 \\ 1 & -4 & 1 \\ 0 & 1 & 0 \end{bmatrix}$$
- **Paso 3: Evaluación de Umbral**
  - Si el valor absoluto $|L(x,y)| > \text{threshold}$ (umbral sintonizado p.ej. a `35.0` o adaptable), el píxel se resalta en **rojo brillante** (`Color32::from_rgb(255, 0, 0)`).

### 2. Modificaciones en el módulo `src/focus.rs`
- Mantener la firma pública:
  ```rust
  pub fn apply_focus_peaking(image: &ColorImage, threshold: f32) -> ColorImage
  ```
- Actualizar los tests unitarios para reflejar el comportamiento del nuevo filtro LoG.
