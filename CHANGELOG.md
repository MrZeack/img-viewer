# Changelog

Cambios por versión según [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/) y [SemVer](https://semver.org/lang/es/). `make bump` genera cada sección desde los commits.

## [0.4.0] - 2026-11-19

### Added

- decode ARW sensor data at full resolution
- weight aesthetic at 70% in the overall score

## [0.3.0] - 2026-11-17

### Added

- move keyboard shortcuts from the status bar to a help modal
- add four display orders and background folder loading
- add automatic 1-10 photo rating with NIMA and technical checks

## [0.2.1] - 2026-11-16

### Added

- add thumbnail grid view with persistent thumbnail index
- replace bookmarks with tags that move images to named folders

### Fixed

- isolate bump test from the caller's TYPE/VERSION
- read CR3 preview and orientation from the container
- read CR3 metadata and preview from the ISO-BMFF container

## [0.2.0] - 2026-11-06

### Added

- add reproducible bump script with changelog
- add --version flag
- add LoG focus peaking, on-demand HQ rendering, composition toggle & config migration (v0.1.18)
- add manual image rotation and EXIF auto-rotation

### Fixed

- resolve clippy warnings blocking make check
- prevent crash on malformed RAW files
