# Explicit Config Migration & Auto-Update Design Specification

## Goal
Implementar un sistema de versionado y migración explícita para `config.toml` en `img-viewer` que actualice automáticamente archivos de configuración antiguos preservando sus valores personalizados y añadiendo nuevas secciones (como `[view]`) sin lanzar errores.

## Design Details

### 1. Versionado del Esquema (`src/config.rs`)
- Agregar campo `pub version: u32` a `Config`:
  - Constante `pub const CURRENT_CONFIG_VERSION: u32 = 2;`
- El campo `version` se serializa en el nivel superior del archivo `config.toml`.

### 2. Algoritmo de Migración (`src/config.rs`)
- En `load_config()`:
  1. Si `toml::from_str::<Config>(&content)` funciona y la versión es `CURRENT_CONFIG_VERSION`, la devuelve directamente.
  2. Si falla el parseo o la versión es menor a `CURRENT_CONFIG_VERSION`:
     - Parsea el contenido a un `toml::Table` (estructura flexible sin error por campos faltantes).
     - Extrae y des-serializa `cache`, `performance` y `browsing` si existen.
     - Inicializa `view` con `ViewConfig::default()`.
     - Establece `version = CURRENT_CONFIG_VERSION`.
     - Sobrescribe `config.toml` en el disco con la configuración migrada y muestra mensaje: `"Migrada configuración config.toml a versión 2"`.
  3. Si no se puede parsear ni migrar el archivo TOML (archivo corrupto), imprime advertencia y crea una configuración por defecto limpia.

### 3. Modificaciones en Tests
- Actualizar `Config::default()` para establecer `version: CURRENT_CONFIG_VERSION`.
- Agregar un test explícito `test_migrate_legacy_config_to_version_2()` que verifique la migración de un archivo TOML legacy (v1) preservando valores personalizados de `cache` y `performance` mientras añade la nueva sección `view`.
