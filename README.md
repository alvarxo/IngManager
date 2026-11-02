# IngManager

Gestión documental local de proyectos de ingeniería (Tauri 2 + React + SQLite).

## Requisitos (desarrollo)

- Node.js 20+ / pnpm
- Rust stable (edition 2021)
- Linux: `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`
- Windows (empaquetado): Visual Studio Build Tools / MSVC, WebView2 SDK vía Tauri CLI

## Desarrollo

```bash
cd ingmanager
pnpm install
pnpm tauri dev
```

Solo frontend (sin comandos Rust):

```bash
pnpm dev
```

## Checks

```bash
pnpm typecheck
pnpm test
pnpm cargo:check
pnpm cargo:test
```

## Empaquetado Windows (NSIS)

Configurado en `src-tauri/tauri.conf.json`:

- Producto / exe: **IngManager** (`IngManager.exe`)
- Instalador: **NSIS** (`bundle.targets: ["nsis"]`)
- WebView2: `downloadBootstrapper` (documentado en `WINDOWS_INSTALL.md`)
- Datos de app: `%LOCALAPPDATA%\IngManager\` (nunca bajo el directorio de instalación)
- Desinstalación: no borra raíz de proyectos ni datos de usuario (hooks NSIS no destructivos)

**No se puede generar un instalador Windows verificado en esta VM Linux.** Use CI:

```text
.github/workflows/windows-release.yml  →  runs-on: windows-latest
artefactos: IngManager_Setup.exe + IngManager.exe
```

Instrucciones de usuario: ver documentación del store `WINDOWS_INSTALL.md` (o el mismo contenido cuando se publique con el repo).

Build local solo en Windows:

```bash
pnpm tauri build --bundles nsis
# nativo: src-tauri/target/release/bundle/nsis/IngManager_*_x64-setup.exe
# bare:   src-tauri/target/release/IngManager.exe
```

## Datos locales

La SQLite y ajustes viven en el perfil del usuario (`dirs::data_local_dir` → carpeta `IngManager`, con compatibilidad de la ruta legacy `ingmanager`). La carpeta raíz de proyectos se guarda en `app_settings` y sobrevive a reinstalaciones.
