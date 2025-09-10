#!/bin/bash

# Image Visualizer Installer
# Instala el visualizador de imágenes en el sistema

set -e

echo "🖼️  Instalador del Visualizador de Imágenes"
echo "=========================================="

# Colores para output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Función para imprimir mensajes con colores
print_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[ADVERTENCIA]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_step() {
    echo -e "${BLUE}[PASO]${NC} $1"
}

# Verificar si el script se está ejecutando desde el directorio correcto
if [ ! -f "img_viewer.py" ]; then
    print_error "No se encontró img_viewer.py en el directorio actual."
    print_error "Ejecuta este instalador desde el directorio del proyecto."
    exit 1
fi

# Verificar permisos de root
if [ "$EUID" -eq 0 ]; then
    print_warning "Ejecutándose como root. Se instalará para todo el sistema."
    INSTALL_DIR="/usr/local/bin"
    DESKTOP_DIR="/usr/share/applications"
else
    print_status "Instalación local para usuario actual."
    INSTALL_DIR="$HOME/.local/bin"
    DESKTOP_DIR="$HOME/.local/share/applications"
fi

print_step "1. Verificando dependencias del sistema..."

# Verificar Python3
if ! command -v python3 &> /dev/null; then
    print_error "Python3 no está instalado."
    print_error "Instala Python3: sudo apt install python3 python3-pip"
    exit 1
fi

# Verificar pip
if ! command -v pip3 &> /dev/null && ! python3 -m pip --version &> /dev/null; then
    print_error "pip no está disponible."
    print_error "Instala pip: sudo apt install python3-pip"
    exit 1
fi

# Verificar tkinter (viene con Python en la mayoría de distribuciones)
if ! python3 -c "import tkinter" 2>/dev/null; then
    print_warning "tkinter no está disponible. Intentando instalar..."
    if command -v apt &> /dev/null; then
        sudo apt update && sudo apt install -y python3-tk
    elif command -v dnf &> /dev/null; then
        sudo dnf install -y python3-pillow-tk
    elif command -v pacman &> /dev/null; then
        sudo pacman -S tk
    else
        print_error "No se pudo instalar tkinter automáticamente."
        print_error "Instala tkinter manualmente para tu distribución."
        exit 1
    fi
fi

print_step "2. Instalando dependencias de Python..."

# Instalar Pillow
if [ "$EUID" -eq 0 ]; then
    pip3 install Pillow>=9.0.0
else
    pip3 install --user Pillow>=9.0.0
fi

print_step "3. Creando directorios de instalación..."

# Crear directorios si no existen
mkdir -p "$INSTALL_DIR"
mkdir -p "$DESKTOP_DIR"

print_step "4. Copiando archivos..."

# Copiar el script principal
cp img_viewer.py "$INSTALL_DIR/img"
chmod +x "$INSTALL_DIR/img"

print_step "5. Creando entrada del menú de aplicaciones..."

# Crear archivo .desktop
cat > "$DESKTOP_DIR/img-viewer.desktop" << EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=Image Viewer
Comment=Visualizador rápido de imágenes JPG con función de selección
Exec=img
Icon=image-x-generic
Terminal=false
Categories=Graphics;Photography;Viewer;
MimeType=image/jpeg;image/jpg;
Keywords=image;photo;viewer;jpg;jpeg;
StartupNotify=true
EOF

# Hacer el archivo desktop ejecutable
chmod +x "$DESKTOP_DIR/img-viewer.desktop"

# Actualizar base de datos de aplicaciones si es posible
if command -v update-desktop-database &> /dev/null; then
    if [ "$EUID" -eq 0 ]; then
        update-desktop-database /usr/share/applications
    else
        update-desktop-database "$HOME/.local/share/applications"
    fi
fi

print_step "6. Configurando PATH (si es necesario)..."

# Verificar si ~/.local/bin está en PATH para instalación local
if [ "$EUID" -ne 0 ]; then
    if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
        print_warning "~/.local/bin no está en tu PATH."
        echo
        echo "Agrega esta línea a tu ~/.bashrc o ~/.profile:"
        echo "export PATH=\"\$HOME/.local/bin:\$PATH\""
        echo
        echo "Luego ejecuta: source ~/.bashrc"
    fi
fi

print_step "7. Creando script de desinstalación..."

# Crear desinstalador
cat > "$INSTALL_DIR/img-viewer-uninstall" << EOF
#!/bin/bash
echo "Desinstalando Image Viewer..."
rm -f "$INSTALL_DIR/img"
rm -f "$DESKTOP_DIR/img-viewer.desktop"
rm -f "$INSTALL_DIR/img-viewer-uninstall"
if command -v update-desktop-database &> /dev/null; then
    if [ "$EUID" -eq 0 ]; then
        update-desktop-database /usr/share/applications
    else
        update-desktop-database "\$HOME/.local/share/applications"
    fi
fi
echo "Image Viewer desinstalado correctamente."
EOF

chmod +x "$INSTALL_DIR/img-viewer-uninstall"

echo
print_status "✅ Instalación completada exitosamente!"
echo
echo "Puedes usar el visualizador de las siguientes formas:"
echo "  • Desde terminal: img"
echo "  • Desde el menú de aplicaciones: busca 'Image Viewer'"
echo "  • Para desinstalar: img-viewer-uninstall"
echo
print_status "Para usar: ve a un directorio con imágenes JPG y ejecuta 'img-viewer'"