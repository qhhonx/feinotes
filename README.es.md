<h1 align="center">Quiet.md</h1>

<p align="center">
  <b>Notas rápidas y tranquilas que viven en tu ordenador.</b><br>
  Una app de notas nativa hecha con Rust + GPUI: carpetas de verdad, Markdown que se formatea solo y una pizarra dentro de tus notas. Gratis y de código abierto.
</p>

<p align="center">
  <a href="README.md">English</a> · <a href="README.zh-CN.md">简体中文</a> · <a href="README.zh-TW.md">繁體中文</a> · <a href="README.ja.md">日本語</a> · <a href="README.ko.md">한국어</a> · Español · <a href="README.fr.md">Français</a>
</p>

<p align="center">
  <a href="https://github.com/s1dashu/quiet-md/releases/latest"><img src="https://img.shields.io/github/v/release/s1dashu/quiet-md?label=release&color=2f6feb" alt="Última versión"></a>
  <a href="https://github.com/s1dashu/quiet-md/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/s1dashu/quiet-md/ci.yml?branch=main&label=CI" alt="Estado de CI"></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey" alt="macOS, Windows y Linux">
  <img src="https://img.shields.io/badge/built%20with-Rust%20%2B%20GPUI-dea584?logo=rust" alt="Hecha con Rust y GPUI">
  <a href="LICENSE"><img src="https://img.shields.io/github/license/s1dashu/quiet-md?color=green" alt="Licencia MIT"></a>
  <img src="https://img.shields.io/badge/status-beta-orange" alt="Beta">
</p>

<p align="center">
  <a href="https://download.quiet.md/latest/quiet-md-macos-arm64.dmg"><img src="docs/images/download-macos.svg" height="56" alt="Descargar para macOS"></a>
  &nbsp;
  <a href="https://download.quiet.md/latest/quiet-md-windows-x64-setup.exe"><img src="docs/images/download-windows.svg" height="56" alt="Descargar para Windows"></a>
  &nbsp;
  <a href="https://download.quiet.md/latest/quiet-md-x86_64.AppImage"><img src="docs/images/download-linux.svg" height="56" alt="Descargar para Linux"></a>
</p>

<p align="center">
  <a href="https://quiet.md/?lang=en"><b>Sitio web</b></a> ·
  <a href="#descarga"><b>Descarga</b></a> ·
  <a href="#funciones">Funciones</a> ·
  <a href="#rendimiento">Rendimiento</a> ·
  <a href="CONTRIBUTING.md">Contribuir</a>
</p>

<p align="center">
  <img src="docs/images/readme-home.webp" width="100%" alt="Quiet.md sobre una pradera en pixel art: la barra lateral muestra las carpetas y el Inicio presenta cada carpeta como una pila de tarjetas de notas con imágenes.">
</p>

> [!NOTE]
> Quiet.md está en **beta** y cambia rápido. Si encuentras un error o tienes una idea, [abre un issue](https://github.com/s1dashu/quiet-md/issues), o mejor aún, envía un pull request.

## Por qué Quiet.md

**📁 Tus notas son archivos, sin más.** Sin cuenta, sin nube y, por defecto, nada sale a la red (salvo la comprobación opcional de actualizaciones y las fuentes que tú provocas: la manuscrita para chino, japonés y coreano la primera vez que una pizarra la necesita, y una fuente de documento descargable cuando la eliges). Cada carpeta de la barra lateral es una carpeta real de tu disco, y cada nota es un archivo con el nombre de su título: `.md` para Markdown y `.xml` para documentos enriquecidos. Sincronízalas con iCloud, Git o lo que prefieras. Si algún día dejas Quiet.md, tus archivos siguen exactamente donde estaban.

**⚡ Nativa, y rápida.** Toda la interfaz la dibujan en la GPU Rust y [GPUI](https://www.gpui.rs/). Sin Electron, sin WebView. En una nota de un millón de caracteres, escribir cuesta unos 5 ms por fotograma, y una pantalla de 120 Hz da 8,3.

**✍️ Escribe y dibuja en el mismo sitio.** Teclea `#`, `-`, `[]` o `>` y el Markdown se formatea solo. Cuando las palabras no bastan, pon una pizarra dentro de la nota. Las notas enriquecidas se guardan en el formato DocxXML de Feishu (Lark), así que al pegar desde Feishu o desde una página web se conserva la estructura.

## Funciones

### Escribir

<p align="center">
  <img src="docs/images/readme-write.webp" width="100%" alt="Una nota larga abierta en Quiet.md, con encabezados, fórmulas en línea y una tabla de concentraciones de iones.">
</p>

Encabezados, listas, tareas, citas, avisos destacados, columnas, tablas con celdas combinadas, código con resaltado de sintaxis, fórmulas LaTeX, imágenes, archivos y portadas. Los atajos de Markdown funcionan mientras escribes y el menú `/` tiene todo lo demás. Menciona otra nota para enlazarla; esa nota mostrará sus referencias inversas.

### Dibujar

<p align="center">
  <img src="docs/images/readme-board.webp" width="100%" alt="Una pizarra en Quiet.md: imágenes y mapas dispuestos con etiquetas manuscritas, cajas y flechas que los unen.">
</p>

Las pizarras son un tablero portado de Excalidraw: formas, flechas que siguen unidas, trazo libre, texto, notas adhesivas, marcos e imágenes. Ponla dentro de una nota o guárdala como archivo `.excalidraw`, que excalidraw.com abre directamente. Haz zoom pellizcando en el trackpad; una pizarra con 10.000 formas sigue fluida.

### Organiza en carpetas de verdad

El Inicio muestra cada carpeta como una pila de sus páginas. Mantén una nota sobre una carpeta y se abrirá sola; las carpetas pueden ir dentro de otras carpetas y los movimientos se pueden deshacer. Fija notas y márcalas como favoritas, busca en todos los títulos y líneas con `⌘ J`, y busca y reemplaza dentro de una nota, también con expresiones regulares. Las notas eliminadas esperan en la papelera. Los archivos y carpetas Markdown que tengas en otras partes del ordenador se pueden abrir en **Archivos de este ordenador** y editar donde están.

### Trae tus notas

<p align="center">
  <img src="docs/images/readme-import.webp" width="100%" alt="La pantalla del primer inicio de Quiet.md, que ofrece importar una bóveda de Obsidian que ha encontrado.">
</p>

En el primer inicio, Quiet.md busca tus bibliotecas de Obsidian, Logseq, iA Writer y Typora y las copia, sin tocar los originales. Los `[[enlaces wiki]]` de Obsidian siguen funcionando: haz clic en uno para abrir la nota a la que apunta. Más adelante, arrastra archivos y carpetas Markdown al Inicio o directamente a una carpeta de la barra lateral. Las notas vuelven a salir como Markdown o PDF, o directas a la impresora.

### Y además

- **Historial de versiones**: cada nota guarda instantáneas al guardarse (20 por defecto, hasta 30). Abre una para ver en su sitio qué cambió, restáurala con un clic o ponle nombre.
- **Notas rápidas**: en macOS, un atajo global (`⌥ Espacio` por defecto) abre una pequeña ventana de nota sobre cualquier app.
- **Tu aspecto**: cuatro estilos integrados —Craft, Feishu, Notion y Obsidian—, cada uno con variante oscura, o tu propio CSS ([docs/THEMES.md](docs/THEMES.md)). Elige la fuente de la interfaz y la de los documentos.
- **Tus teclas**: pulsa `⌘ /` para ver todos los atajos y reasignar cualquiera. En Windows y Linux, Ctrl sustituye a ⌘ y Alt a ⌥.
- **Tu idioma**: inglés, chino simplificado, chino tradicional, japonés, coreano, español, francés, árabe y turco. El árabe une sus letras y se escribe de derecha a izquierda.
- **Limpieza**: Ajustes › Biblioteca encuentra las imágenes y archivos que ya no usa ninguna nota y los retira cuando lo confirmas.
- **Actualizaciones**: las versiones nuevas se instalan desde la propia app tras verificar su firma. Las comprobaciones automáticas se pueden desactivar.

## Descarga

<p align="center">
  <a href="https://download.quiet.md/latest/quiet-md-macos-arm64.dmg"><img src="docs/images/download-macos.svg" height="56" alt="Descargar para macOS"></a>
  &nbsp;
  <a href="https://download.quiet.md/latest/quiet-md-windows-x64-setup.exe"><img src="docs/images/download-windows.svg" height="56" alt="Descargar para Windows"></a>
  &nbsp;
  <a href="https://download.quiet.md/latest/quiet-md-x86_64.AppImage"><img src="docs/images/download-linux.svg" height="56" alt="Descargar para Linux"></a>
</p>

| Plataforma | Paquete | Requisitos |
|---|---|---|
| **macOS** (Apple Silicon) | [quiet-md-macos-arm64.dmg](https://download.quiet.md/latest/quiet-md-macos-arm64.dmg) | macOS 13 o posterior |
| **Windows** (x64) | [quiet-md-windows-x64-setup.exe](https://download.quiet.md/latest/quiet-md-windows-x64-setup.exe) | Windows 10 versión 1809 o posterior |
| **Linux** (x86_64, experimental) | [quiet-md-x86_64.AppImage](https://download.quiet.md/latest/quiet-md-x86_64.AppImage) | glibc 2.35 o posterior (Ubuntu 22.04 o más reciente) |
| **macOS** (Intel) | [quiet-md-macos-x86_64.dmg](https://download.quiet.md/latest/quiet-md-macos-x86_64.dmg) | macOS 13 o posterior |

- **macOS**: abre el `.dmg`, arrastra `quiet.md.app` al acceso directo de Aplicaciones que hay al lado y ábrela desde Aplicaciones. La app y la imagen de disco están firmadas y notarizadas por Apple. El `.zip` de la página de Releases contiene la misma app; es lo que descargan las actualizaciones integradas.
- **Windows**: ejecuta el instalador. Por defecto se instala solo para el usuario actual, y desinstalarla nunca borra tus notas. Si aparece SmartScreen en el primer arranque, haz clic en **Más información → Ejecutar de todas formas**.
- **Linux** (experimental): `chmod +x quiet-md-*.AppImage` y ejecútala; sin FUSE, añade `--appimage-extract-and-run`. La AppImage admite actualizaciones desde la app a partir de la 0.1.10; las versiones de Linux anteriores necesitan una instalación manual una sola vez. Por ahora solo se ha probado a mano en Omarchy con Wayland; consulta las [notas para Linux](CONTRIBUTING.md#linux-experimental).

También puedes descargarla desde [Quiet.md](https://quiet.md/?lang=en). Todas las versiones y sus sumas SHA-256 están en [GitHub Releases](https://github.com/s1dashu/quiet-md/releases).

En el primer arranque eliges el idioma, dónde guardar la biblioteca y el atajo de notas rápidas, y puedes copiar tus notas de Obsidian, Logseq, iA Writer o Typora. Una biblioteca vacía recibe la guía **Getting started**: un recorrido de diez minutos por las funciones principales. Nunca sobrescribe las notas que ya tengas.

## Rendimiento

Tiempo de procesamiento por fotograma a lo largo de 100 operaciones seguidas en un documento mixto de **1 millón de caracteres** y 549 capítulos:

| Operación | P50 | P95 |
|---|---:|---:|
| ⌨️ Escribir | **5.06 ms** | 5.78 ms |
| ⌫ Retroceso | **4.99 ms** | 5.55 ms |
| 🖱️ Desplazarse | **2.08 ms** | 4.14 ms |

Una pantalla de 120 Hz da 8.3 ms a cada fotograma. Incluso en el P95 de un documento de un millón de caracteres sobra margen.

| | |
|---|---|
| Descarga para macOS | ~18 MB |
| Instalador para Windows | ~14 MB |
| En ejecución | Sin WebView, sin Electron, sin servicios en segundo plano |

<sub>Apple M5 Pro / 48 GiB, macOS 26.5, compilación release. Metodología y scripts para reproducirlo: [docs/PERFORMANCE.md](docs/PERFORMANCE.md).</sub>

## Colaboradores

Gracias a todas las personas que han ayudado a construir Quiet.md. Los issues y pull requests son bienvenidos: cómo compilar, probar y las convenciones de código están en [CONTRIBUTING.md](CONTRIBUTING.md).

[![Colaboradores](https://raw.githubusercontent.com/s1dashu/quiet-md/contributors-assets/docs/images/contributors.svg?v=1)](https://github.com/s1dashu/quiet-md/graphs/contributors)

## Novedades recientes

- **0.1.14** (22 nov): buscar y reemplazar con expresiones regulares. Los `[[enlaces wiki]]` de Obsidian abren la nota a la que apuntan. Interfaz en turco, y el árabe se muestra de derecha a izquierda. Elige la fuente de la interfaz y la de los documentos, y retira las imágenes y archivos que ninguna nota usa. La búsqueda sigue el ritmo en bibliotecas grandes y borrar muchas notas es más rápido. Un ajuste para que Retroceso vuelva al título; en Windows y Linux, Copiar como Markdown pasa a ser Ctrl+Shift+M.
- **0.1.13** (17 nov): pizarras, portadas de Excalidraw, como bloque en una nota o como archivo `.excalidraw`, fluidas incluso con 10.000 formas. Historial de versiones en cada nota (… › Historial de versiones). Interfaz en francés y árabe. Copiar como Markdown, copiar la ruta de un archivo y cambiar la carpeta de la biblioteca en Ajustes. Las imágenes pegadas desde la web se guardan en la biblioteca. Menos memoria en páginas con muchas imágenes y desplazamiento más fluido en la barra lateral.
- **0.1.12** (14 nov): arrastra archivos desde el Finder, o desde Archivos de este ordenador, directamente a una carpeta de la barra lateral o entre filas; toda la página se ilumina como zona para soltar. Tarjetas de Inicio con esquinas continuas, el menú Abrir en el Finder, rutas que empiezan en Mis documentos, un ancho mínimo de ventana que deja sitio a la nota y una barra lateral opaca en pantalla completa. Corrige un bloqueo al crear un documento dentro de una carpeta.
- **0.1.11** (13 nov): la biblioteca pasa a ser carpetas reales, con archivos nombrados por su título; las bibliotecas anteriores se trasladan solas, con copia de seguridad. Un nuevo primer arranque para elegir idioma, ubicación de la biblioteca y atajo de notas rápidas, e importar notas de Obsidian y otras apps de Markdown. Interfaz en japonés, español y coreano; fórmulas en bloque en Markdown; los documentos se abren donde los dejaste; arranque más rápido y menos memoria.
- **0.1.10** (10 nov): elige el tipo de documento en el menú **Nuevo**, desplaza las tablas anchas en horizontal, conserva la alineación de columnas de Markdown y mantén visible el cursor en los campos largos.
- **0.1.9** (8 nov): corrige Cmd+Q en macOS, los botones de ventana que no respondían en Windows y la selección de texto oculta por los colores de resaltado.
- **0.1.8** (9 nov): edita archivos Markdown locales en su sitio, importa carpetas conservando los `.md` y organiza las notas en carpetas anidadas arrastrando, con deshacer. Menos memoria para imágenes y vistas previas, cambio de tamaño de tablas más estable y menús pulidos. El primer arranque elige un idioma del sistema compatible. Paquete AppImage experimental para Linux.
- **0.1.7** (6 nov): guardado en varias ventanas, recuperación de conflictos y escritura en disco más seguros; acceso a archivos restringido al exportar; las exportaciones a Markdown conservan los adjuntos en línea. Corregido el borrado de texto al escribir chino con Weasel en Windows.
- **0.1.6** (6 nov): cambio de ventana y escritura más fluidos, tablas más anchas, guardado periódico mientras escribes sin parar y Option+Espacio para notas rápidas desde cualquier lugar en macOS. Windows añade pegar imágenes, exportar a PDF e imprimir, y corrige el borrado por palabras.
- **0.1.5** (5 nov): la app pasa a llamarse Quiet.md; tu biblioteca y tus notas siguen donde estaban.
  - Gestiona varios documentos a la vez desde **Inicio**: duplícalos, muévelos o elimínalos juntos.
  - Vacía la papelera de una vez; las flechas recorren los iconos del menú de inserción; eliminar todos los documentos ya no crea uno vacío.
  - Tras actualizar desde la app, la barra lateral muestra una vez qué ha cambiado en la nueva versión.
  - Windows: los atajos y la guía integrada indican Ctrl / Alt, y si el arranque falla se explica el motivo y se deja un registro.
- **0.1.4** (5 nov):
  - Actualizaciones desde la app: cuando sale una versión nueva aparece el botón **Actualizar** en la parte superior de la barra lateral. La descarga se verifica con su firma y un reinicio termina la instalación; antes se guardan los documentos abiertos. La búsqueda automática se puede desactivar en **Ajustes › General**. Solo descarga un manifiesto de versión y no envía ningún dato. Las versiones 0.1.3 y anteriores necesitan descargar la 0.1.4 a mano una vez.
  - Modo oscuro: elige Sistema, Claro u Oscuro en **Ajustes › Apariencia**. Los temas integrados y los personalizados tienen aspecto oscuro; la exportación y la impresión siguen en claro.
- **0.1.3** (4 nov): el texto de la barra lateral vuelve a leerse bien cuando macOS está en modo oscuro.
- **0.1.2** (4 nov): los menús y atajos estándar de macOS (Ocultar, Minimizar, Zoom, Pantalla completa); al hacer clic en el icono del Dock se vuelve a abrir una ventana después de cerrar la última. Corregidos algunos caracteres chinos que se mostraban con glifos incorrectos.
- **0.1.1** (3 nov): primera versión para Windows x64, instalada por usuario y que conserva tus notas al desinstalar. Pulidas la interfaz en chino e inglés y la guía integrada.

Las notas completas de cada versión están en [Releases](https://github.com/s1dashu/quiet-md/releases).

## Licencia

El código se publica bajo la licencia [MIT](LICENSE). El código, las fuentes y los recursos de terceros se enumeran en [THIRD_PARTY.md](THIRD_PARTY.md).

Lo siguiente no está cubierto por la licencia MIT y debe sustituirse en los forks:

- Los iconos de Nucleo de `assets/icons/` (71) y `assets/editor-icons/` (23) están bajo una licencia comercial de Nucleo. No los extraigas ni los redistribuyas por separado.
- El nombre “Quiet.md” y el icono de la app.
