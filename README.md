# IngManager

Gestión documental local de proyectos de ingeniería (Tauri 2 + React + SQLite).

## Requisitos

- Node.js 20+ / pnpm
- Rust stable (edition 2021)
- Linux: `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`

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

## Fase 1

- Setup de carpeta raíz
- Crear / listar / archivar proyectos (códigos AANN)
- Carpetas estándar físicas
- Layout oscuro denso
- Indexación inicial + abrir/revelar archivos bajo la raíz
