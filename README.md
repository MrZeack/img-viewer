# Image Visualizer

Un visualizador de imágenes rápido para Linux diseñado para seleccionar las mejores fotos después de una sesión fotográfica.

## Características

- Visualiza imágenes JPG del directorio actual
- Navegación con teclas de flecha (← →)
- Zoom con rueda del ratón
- Movimiento de imagen arrastrando con click izquierdo
- Función "Like" (tecla L) que mueve imágenes al subdirectorio "best/"

## Instalación

### Instalación automática (recomendada)

```bash
./install.sh
```

El instalador se encargará de:
- Verificar dependencias del sistema
- Instalar Pillow automáticamente
- Crear entrada en el menú de aplicaciones
- Configurar el comando `img-viewer`

### Instalación manual

1. Instalar dependencias:
```bash
pip install -r requirements.txt
dnf install python3-pillow-tk
```

2. Ejecutar el visualizador:
```bash
./img_viewer.py
```

## Controles

- **← →**: Navegar entre imágenes
- **Rueda del ratón**: Zoom in/out
- **Click izquierdo + arrastrar**: Mover imagen
- **L**: Like - mover imagen actual al directorio "best/"
- **Esc**: Salir

## Uso

1. Coloca el script en el directorio que contiene tus imágenes JPG
2. Ejecuta `./img_viewer.py`
3. Navega por las imágenes y usa 'L' para marcar las mejores
4. Las imágenes marcadas se moverán automáticamente al subdirectorio "best/"