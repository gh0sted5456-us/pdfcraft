# PdfCraft app icon

<img src="pdfcraft-small.svg" alt="PdfCraft app icon: the original engraved lion on transparent surroundings" width="128">

Personal version 0.13.1 keeps the original engraved artwork with transparent surroundings.
The theme palettes are by @gh0sted5456-us (2026-10-23); the lion's paths are unchanged.
Window icons, Home and About show a dark lion in light mode and a light lion in dark mode.
Installed launcher and Explorer icons use the static dark variant; Windows/Linux launchers
do not follow this application's theme preference.

**Creature:** a lion, in a frontal head-and-shoulders portrait, mane running off the bottom of the tile.

**Style:** an engraving (woodcut-weight line work) portrait in the Crafting Apps "owl template" framing:
no surrounding colour tile, the animal looking at the viewer and filling the available space.

**Palette:** exactly two colours.

| Colour | Hex | Used for |
|---|---|---|
| Black | `#0b0b0c` | silhouette in light mode, engraving in dark mode |
| Paper | `#efe9dc` | engraving in light mode, silhouette in dark mode |

**Tile:** `viewBox="0 0 512 512"`, a rounded square with `rx=112` that clips everything. Windows and Linux
icons keep the transparent surround. macOS icons put it on Apple's grid (an 824 px body centred on a transparent
1024 px canvas).

**Provenance:** the project owner's original drawing, made in ArtCraft (2880 px, keyed to the palette), then
vectorised with craftrules `assets/logo-options/_tools/vectorize_tile.py` (potrace; no filtering or
warping). The source drawing is kept in craftrules at `assets/app-icons/pdfcraft/source.png`, not here.
Licence: [LICENSE.txt](LICENSE.txt) (`MIT OR Apache-2.0`, like the repo).

## Files

| File | What it is |
|---|---|
| `pdfcraft.svg` | the master vector (traced at 2048 px); every PNG, `.ico` and `.icns` is rendered from it |
| `pdfcraft-small.svg` | a lighter vector (traced at 1024 px) for places where size matters, such as this README |
| `pdfcraft-dark.png` | the inverted palette for the dark interface and native window icon |
| `pdfcraft-1024.png` | 1024 px on Apple's grid; also the runtime Dock icon on macOS |
| `pdfcraft.icns` | macOS icon (16–1024 px) |
| `pdfcraft.ico` | Windows icon (16–256 px), embedded in `pdfcraft.exe` by `apps/pdfcraft/build.rs` |
| `pdfcraft-document.svg` | Windows PDF file icon master: a Lucide document outline, black badge and outlined PDF lettering; no font dependency |
| `pdfcraft-document.ico` | Windows PDF file icon (16–256 px), embedded as resource 2 in `pdfcraft.exe` |
| `hicolor/<n>x<n>/apps/ai.storyteller.pdfcraft.png` | Linux hicolor theme, 16–512 px; the 256 px one is the runtime icon on Windows and Linux |
| `hicolor/scalable/apps/ai.storyteller.pdfcraft.svg` | Linux scalable icon (copy of the master) |

Where it shows: `apps/pdfcraft/src/main.rs` sets the window icon (Dock, taskbar, Alt-Tab, launcher) and the
Wayland app id `ai.storyteller.pdfcraft`; `packaging/linux/ai.storyteller.pdfcraft.desktop` names the
hicolor icon.

Windows PDF files use resource 2 (`"pdfcraft.exe",-2`) when PdfCraft is chosen as the default
reader; the app keeps resource 1. The document outline is adapted from Lucide `file-text`
1.49.0 (ISC); its badge and outlined lettering are original (`MIT OR Apache-2.0`), covered by
[LICENSE-document.txt](LICENSE-document.txt). Both window/document icons are embedded. The MSI
also installs a versioned copy of the app ICO for Explorer's native lower-right thumbnail badge,
so an update does not reuse the previous release's cached icon path. The thumbnail provider
supplies first-page previews. Home and About also use the same app icon.

## Regenerate

```sh
packaging/icons.sh        # needs resvg and python3; iconutil (macOS) for the .icns
cargo xtask assets        # then update the sha256 values in ATTRIBUTION.toml and run with --write
```
