<p align="center">
  <img src="public/icon-transparent.png" alt="Buddy Switch" width="120" />
</p>

<h1 align="center">Buddy Switch</h1>

<p align="center">
  <b>Estación de trabajo multicuenta WorkBuddy · CodeBuddy · TraeWork · Qoder</b><br />
  «¿Con qué cuenta estoy dentro?» pasa a ser un solo clic
</p>

<p align="center">
  <img alt="Plataformas" src="https://img.shields.io/badge/Plataformas-macOS_%7C_Windows_%7C_Linux-2ea043" />
  <a href="https://github.com/NextAgentX/qoder-trae-workbuddy-switch/releases/latest"><img alt="Versión" src="https://img.shields.io/github/v/release/NextAgentX/qoder-trae-workbuddy-switch?label=Versi%C3%B3n" /></a>
  <a href="./LICENSE"><img alt="Licencia" src="https://img.shields.io/badge/Licencia-PolyForm_Noncommercial-4c8bf5" /></a>
  <a href="https://github.com/NextAgentX/qoder-trae-workbuddy-switch/stargazers"><img alt="Stars" src="https://img.shields.io/github/stars/NextAgentX/qoder-trae-workbuddy-switch?style=social" /></a>
</p>

<p align="center">
  <a href="README.md">简体中文</a> ·
  <a href="README.en.md">English</a> ·
  <a href="README.ja.md">日本語</a> ·
  <a href="README.ko.md">한국어</a> ·
  <a href="README.ru.md">Русский</a> ·
  <a href="README.fr.md">Français</a> ·
  <strong>Español</strong> ·
  <a href="README.pt.md">Português</a>
</p>

# Buddy Switch

Una estación de trabajo multicuenta **totalmente local**: reúne las cuentas de cuatro líneas de producto — **WorkBuddy / CodeBuddy / TraeWork / Qoder** — y **6 ámbitos** en un solo lugar. Inicio de sesión por QR o código de dispositivo, cambio de cuenta en un clic, registros diarios y créditos, analítica de uso, sincronización de copias de sesión y publicación de tu cuota de modelos a otras herramientas de IA locales mediante endpoints compatibles con OpenAI / Anthropic.

|  |  |
| --- | --- |
| 🪪 **6 ámbitos en un solo sitio** | Tres líneas de producto × CN / Global, cada una con su propio almacén de cuentas. El cambio muestra progreso y se revierte solo si falla la escritura. |
| ✅ **La lista de pendientes se escribe sola** | El resumen enumera «reiniciar sesión / registro de hoy / créditos por caducar / cliente no iniciado», con la acción en la propia fila. |
| 📊 **Uso y límites a la vista** | Tokens, créditos, límites «cuenta × modelo» y qué copia de sesión va atrás o divergió: todo en una pantalla. |
| 🔌 **La cuota también alimenta otras herramientas** | Se expone como endpoint compatible con OpenAI / Anthropic: Cursor, Claude Code, OpenWebUI. |

<p align="center">
  <a href="https://github.com/NextAgentX/qoder-trae-workbuddy-switch/releases/latest"><img alt="Descargar" src="https://img.shields.io/badge/Descargar-Releases-2ea043?style=for-the-badge" /></a>
  <a href="https://github.com/NextAgentX/qoder-trae-workbuddy-switch/stargazers"><img alt="Dar una estrella" src="https://img.shields.io/badge/Dar_una_estrella-Star-f5c542?style=for-the-badge" /></a>
  <a href="CONTRIBUTING.en.md"><img alt="Contribuir" src="https://img.shields.io/badge/Contribuir-Pull_Request-4c8bf5?style=for-the-badge" /></a>
</p>


> ⚠️ **Herramienta de terceros no oficial** (source-available, licencia PolyForm Noncommercial). Sin afiliación, autorización ni respaldo de esos productos ni de sus titulares. Lee y escribe los datos de autenticación de los clientes locales, envía peticiones automáticas según tu programación y puede reenviar tu cuota por un puerto local. Lee el **[aviso legal](docs/policies/DISCLAIMER.en.md)** antes de usarla.

## Capturas


<table>
  <thead>
    <tr><th>Resumen (claro)</th><th>Resumen (oscuro)</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/overview-light.png" alt="Resumen, tema claro" /></td>
      <td><img src="docs/images/overview-dark.png" alt="Resumen, tema oscuro" /></td>
    </tr>
  </tbody>
</table>

<table>
  <thead>
    <tr><th>Cuentas de WorkBuddy (claro)</th><th>Cuentas de WorkBuddy (oscuro)</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/workbuddy-accounts-light.png" alt="Cuentas de WorkBuddy, tema claro" /></td>
      <td><img src="docs/images/workbuddy-accounts-dark.png" alt="Cuentas de WorkBuddy, tema oscuro" /></td>
    </tr>
  </tbody>
</table>

<table>
  <thead>
    <tr><th>Uso · tendencia (claro)</th><th>Uso · tendencia (oscuro)</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/usage-trend-light.png" alt="Tendencia de uso, tema claro" /></td>
      <td><img src="docs/images/usage-trend-dark.png" alt="Tendencia de uso, tema oscuro" /></td>
    </tr>
  </tbody>
</table>

<table>
  <thead>
    <tr><th>Pasarela (claro)</th><th>Pasarela (oscuro)</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/api-service-light.png" alt="Pasarela, tema claro" /></td>
      <td><img src="docs/images/api-service-dark.png" alt="Pasarela, tema oscuro" /></td>
    </tr>
  </tbody>
</table>

<table>
  <thead>
    <tr><th>Pasarela Qoder (claro)</th><th>Pasarela Qoder (oscuro)</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/qoder-gateway-light.png" alt="Pasarela Qoder (claro)" /></td>
      <td><img src="docs/images/qoder-gateway-dark.png" alt="Pasarela Qoder (oscuro)" /></td>
    </tr>
  </tbody>
</table>

<details>
<summary><b>Más capturas (sesiones / ajustes / TraeWork / Qoder)</b></summary>

<table>
  <thead>
    <tr><th>Sesiones (claro)</th><th>Sesiones (oscuro)</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/sessions-light.png" alt="Sesiones (claro)" /></td>
      <td><img src="docs/images/sessions-dark.png" alt="Sesiones (oscuro)" /></td>
    </tr>
  </tbody>
</table>

<table>
  <thead>
    <tr><th>Ajustes (claro)</th><th>Ajustes (oscuro)</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/settings-light.png" alt="Ajustes (claro)" /></td>
      <td><img src="docs/images/settings-dark.png" alt="Ajustes (oscuro)" /></td>
    </tr>
  </tbody>
</table>

<table>
  <thead>
    <tr><th>Cuentas de TraeWork (claro)</th><th>Cuentas de TraeWork (oscuro)</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/trae-accounts-light.png" alt="Cuentas de TraeWork (claro)" /></td>
      <td><img src="docs/images/trae-accounts-dark.png" alt="Cuentas de TraeWork (oscuro)" /></td>
    </tr>
  </tbody>
</table>

<table>
  <thead>
    <tr><th>Cuentas de Qoder (claro)</th><th>Cuentas de Qoder (oscuro)</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/qoder-accounts-light.png" alt="Cuentas de Qoder (claro)" /></td>
      <td><img src="docs/images/qoder-accounts-dark.png" alt="Cuentas de Qoder (oscuro)" /></td>
    </tr>
  </tbody>
</table>

</details>

> Imágenes generadas por `scripts/gen-readme-shots.mjs` desde la compilación de demostración (`VITE_DEMO_MODE=1`); cuentas, créditos y peticiones son ficticios. Tras cambiar la interfaz basta con volver a ejecutarlo.

## Vista general

```mermaid
flowchart TB
    subgraph SRC["3 líneas de producto · 6 ámbitos"]
        direction LR
        WB["WorkBuddy<br/>CN / Global"]
        TR["TraeWork<br/>CN / Global"]
        QD["Qoder<br/>CN / Global"]
    end
    subgraph CORE["Núcleo local · ~/.buddy-switch/"]
        direction LR
        AC["Almacenes de cuentas<br/>credenciales · papelera"]
        AU["Automatización<br/>registros · keepalive · rotación"]
        US["Uso<br/>tokens / créditos"]
        SE["Copias de sesión<br/>huella de contenido · sincronización aditiva"]
        GW["Pasarela de cuota<br/>compatible con OpenAI / Anthropic"]
    end
    WB --> AC
    TR --> AC
    QD --> AC
    AC --- AU --- US --- SE
    US --> GW
    GW --> OUT["Otras herramientas de IA locales<br/>Cursor · Claude Code · OpenWebUI"]
```

**Tres reglas firmes** en todo el producto: los datos se quedan en esta máquina, las credenciales solo se escriben en el estado de sesión del cliente (sin inicio de sesión proxy ni cambios en el protocolo del proveedor) y toda escritura se respalda antes.

## Tres formas

| Forma | Cómo obtenerla | Nota |
| --- | --- | --- |
| **Aplicación de escritorio** | Descargar el instalador desde [GitHub Releases](https://github.com/NextAgentX/qoder-trae-workbuddy-switch/releases/latest) | Empaquetada con Tauri; recomendada para el día a día |
| **webui** | Compilar desde el código (abajo) | El mismo bundle del frontend, servido por HTTP local |
| **Demo en línea** | [GitHub Pages](https://nextagentx.github.io/qoder-trae-workbuddy-switch/) | Demo de solo lectura; cuentas y uso son ficticios |

> El paquete npm aún no está publicado, así que `npm i -g` no lo instalará; para la forma webui usa la compilación desde el código.

## Inicio rápido

**Aplicación de escritorio**: descarga el instalador de tu plataforma desde Releases.

| Plataforma | Instalador | Instalación |
| --- | --- | --- |
| macOS Apple Silicon (arm64) | `BuddySwitch_<versión>_aarch64.dmg` | Abre el DMG y arrastra `BuddySwitch.app` a Aplicaciones |
| macOS Intel (x86_64) | `BuddySwitch_<versión>_x86_64.dmg` | Igual que arriba |
| Windows x64 | `BuddySwitch_<versión>_x64-setup.exe` | Ejecuta el instalador |
| Linux x64 | `BuddySwitch_<versión>_amd64.deb` / `.AppImage` | Instala el `.deb` o da permisos de ejecución al AppImage |

**webui (compilar desde el código)**: el bundle del frontend se incrusta en el servidor **en tiempo de compilación** (`rust-embed`), así que el orden es obligatorio.

```bash
npm install
npm run build                                    # 1) frontend → dist/
cargo build --release -p buddy-switch-server     # 2) compilar el servidor (incrusta dist/)
./target/release/buddy-switch                    # 3) servir y abrir el navegador (127.0.0.1:57890)
```

```bash
buddy-switch              # servir + abrir navegador
buddy-switch serve        # solo servir (--port para el puerto)
buddy-switch status       # cuenta actual en la terminal
buddy-switch version      # versión
```

<details>
<summary><b>macOS: «no se puede verificar al desarrollador» o «sin permisos» al cambiar de cuenta</b></summary>

- No se puede verificar al desarrollador: en Finder, clic con Control sobre la app → Abrir, o Ajustes del Sistema → Privacidad y seguridad → «Abrir igualmente».
- El cambio de cuenta escribe el archivo de autenticación del cliente: concede a BuddySwitch **Gestión de apps** (o **Acceso total al disco**) en Privacidad y seguridad, y reinicia la app. La «comprobación de permisos» de los ajustes lo verifica en cualquier momento.
- En la forma webui, los permisos son los del proceso de terminal que inició el servicio.

</details>

## 6 ámbitos (plataforma × región)

El selector de la esquina superior izquierda decide qué conjunto de cuentas gestionas. El ámbito vive en la URL (`/s/wb-cn/accounts`), así que recargar, compartir el enlace o ir atrás/adelante mantiene la posición; `⌘K` / `Ctrl+K` busca en todos los ámbitos y `⌘1`–`⌘6` cambia directamente.

| Ámbito | Plataforma | Región | Clientes gestionados |
| --- | --- | --- | --- |
| `wb-cn` | WorkBuddy | CN | WorkBuddy de escritorio, CodeBuddy CN IDE / CLI, extensiones CodeBuddy para VS Code / JetBrains |
| `wb-global` | WorkBuddy | Global | Los mismos clientes (versiones globales) |
| `trae-cn` | TraeWork | CN | Trae CN |
| `trae-global` | TraeWork | Global | Trae |
| `qoder-cn` | Qoder | CN | Qoder CN de escritorio / CLI |
| `qoder-global` | Qoder | Global | Qoder de escritorio / CLI |

Los seis ámbitos son **totalmente independientes**: almacenes de cuentas, clientes, configuración y claves de pasarela separados. Las dos líneas antiguas de Trae (TraeWork / TraeCode) se fusionaron aguas arriba en un solo cliente; los identificadores antiguos solo se mantienen por compatibilidad.

## 6 destinos de la barra lateral

Idénticos punto por punto en las tres plataformas: **Resumen · Cuentas · Uso · Sesiones · Pasarela · Ajustes**. Las entradas se filtran por **capacidad** (no por rama de plataforma): sin capacidad no hay entrada, y un acceso directo por URL devuelve un estado vacío estructurado.

| Destino | En una línea |
| --- | --- |
| **Resumen** | Lo que requiere acción (reiniciar sesión / registro de hoy / créditos por caducar / cliente no iniciado) + tarjetas de estadísticas + gráfico de tokens de 14 días + vistazo a otros ámbitos |
| **Cuentas** | Vistas de tarjetas y tabla, ranuras de programa, inicio por QR o código de dispositivo, importación local, registros, papelera (30 días), registro de límites (solo WorkBuddy) |
| **Uso** | Una sola estación con cuatro vistas: tendencia · modelos · sesiones · créditos; una única barra de parámetros (rango + origen de datos) |
| **Sesiones** | Copias de una misma sesión entre cuentas: quién va atrás, quién divergió, a quién sincronizar (**solo WorkBuddy**) |
| **Pasarela** | Pasarelas independientes (una por ámbito) y pasarela unificada (un proveedor para todos los ámbitos) |
| **Ajustes** | Nueve secciones: general · cuenta e inicio de sesión · registros y créditos · pasarela · programaciones · cliente · permisos y diagnóstico · registros de actividad · copia y restauración |

### Matriz de capacidades

| Capacidad | WorkBuddy | TraeWork | Qoder |
| --- | :---: | :---: | :---: |
| Almacén de cuentas / inicio / importación | ✅ QR + importación | ✅ inicio web + JWT | ✅ código de dispositivo + PAT |
| Ranuras de programa | ✅ 5 (4 conmutables + 1 solo lectura) | ✅ 1 (cliente fusionado) | ✅ 2 (escritorio / CLI) |
| Registros / créditos | ✅ con viaje automático y rotación | ✅ | ✅ registro diario + plan / cuota |
| Analítica de uso | ✅ tokens + créditos | ✅ tokens + créditos | ⏳ origen de datos pendiente |
| Sincronización de copias | ✅ huella de contenido + aditiva | — | — |
| Pasarela de cuota | ✅ `:57891` | ✅ `:57894` | ✅ `:57895` |

## Cómo se cambia de cuenta

```mermaid
flowchart LR
    A["Elegir cuenta"] --> B["Respaldar estado de sesión"] --> C["Cerrar el cliente"] --> D["Escribir la cuenta destino"] --> E["Verificar y reiniciar"]
    D -->|"opcional"| F["Además: copiar sesiones<br/>fusionar memoria y conectores"]
```

- **Respaldar antes de escribir**: archivos de autenticación, instantáneas de sesión de Trae y sobrescrituras de sesión dejan siempre una copia para revertir.
- **Verificar después de escribir**: una escritura fallida se revierte automáticamente; si el cliente está en ejecución se rechaza la escritura, para que no reescriba tu estado con el que tiene en memoria.
- **Borrado suave**: la cuenta va a la papelera 30 días y puede restaurarse; el borrado definitivo elimina también credenciales e instantáneas.

## Pasarela de cuota de modelos

Expone la cuota de la cuenta actual a las herramientas locales: compatibilidad OpenAI (`/v1/models`, `/v1/chat/completions`, `/v1/responses`) y Anthropic (`/v1/messages`), ambas con streaming SSE. Por defecto solo escucha en loopback.

```mermaid
flowchart LR
    C["Herramientas de IA locales"] -->|"sk-wb-…"| G1[":57891<br/>pasarela WorkBuddy"]
    C -->|"sk-trae-…"| G2[":57894<br/>pasarela Trae"]
    C -->|"sk-qoder-…"| G3[":57895<br/>pasarela Qoder"]
    C -->|"sk-uni-…"| G4[":57892<br/>pasarela unificada"]
    G1 --> P1["pools wb-cn / wb-global"]
    G2 --> P2["pools trae-cn / trae-global"]
    G3 --> P3["pools qoder-cn / qoder-global"]
    G4 -->|"el nombre del modelo lleva prefijo de espacio de nombres"| P1
    G4 --> P2
    G4 --> P3
```

- **Cuatro escuchas independientes**: los puertos forman un solo bloque **`5789x`** — `57890` interfaz · `57891` WorkBuddy · `57892` unificada · `57893` hook de sesión · `57894` Trae · `57895` Qoder. Apagar la pasarela unificada no afecta a ningún cliente existente; las rutas `/v1/*` se solapan literalmente, así que **no pueden** fusionarse en un único Router.
- **Los prefijos de clave son una sola familia**: `sk-wb-` · `sk-trae-` · **`sk-qoder-`** · `sk-uni-` — el prefijo ya dice qué puerto configurar.
- **Ciclo de vida de la clave**: revocar · restaurar · eliminar (solo se eliminan las revocadas). El texto plano se devuelve una vez al crearla; el almacén guarda sha256 y los 4 primeros/últimos caracteres.
- **Pool de cuentas**: elige cuentas por saldo y estado de límites, excluye las inutilizables, sondea límites «cuenta × modelo» y las reactiva con un clic.

La **pasarela unificada** resuelve el problema de modelos homónimos que se pisan entre plataformas: el nombre pasa a `<espacio de nombres>:<id del modelo>`, y los espacios de nombres derivan del registro de ámbitos (hoy seis: `wb-cn` / `wb-global` / `trae-cn` / `trae-global` / `qoder-cn` / `qoder-global`).

| Regla | Comportamiento |
| --- | --- |
| Nombre de modelo sin prefijo | **Siempre 400** con la lista de prefijos válidos (sin adivinar un valor por defecto y acabar en el pool equivocado) |
| Análisis del espacio de nombres | **Solo coincidencia exacta del segmento completo** (`trae` / `traework` / `traecode` son prefijos entre sí) |
| Una clave, varias plataformas | Una misma clave `sk-uni-` puede autorizar varias plataformas |
| `/v1/responses` en `trae-*` | **501**, nunca una degradación silenciosa a chat |
| Estado por defecto | **Apagada**; se activa en la sección «Pasarela unificada» de la página de pasarela |

> Arquitectura: [UNIFIED-GATEWAY-ARCH.md](docs/specs/UNIFIED-GATEWAY-ARCH.md) · Fragmentos de configuración: [guides/GATEWAY-CLIENT-INTEGRATION.md](docs/guides/GATEWAY-CLIENT-INTEGRATION.md)

## Automatización

Las programaciones admiten hora **con precisión de minuto** (p. ej. `09:37`), varias horas y **desfase aleatorio** (±5/15/30 minutos) para repartir las ejecuciones.

| Lado | Tareas |
| --- | --- |
| WorkBuddy (6) | Registro · viaje del gato · mapa de actividad · keepalive del token · vuelta al cole · ave nocturna |
| Trae (2) | Registro · renovación automática de JWT |

<details>
<summary><b>Política de rotación automática (WorkBuddy, evita caducar créditos)</b></summary>

Un bucle en segundo plano fija la cuenta por defecto de **los siguientes arranques de CodeBuddy CLI** en la más urgente (caduca antes y aún tiene créditos). Orden de decisión:

1. Cuenta válida (consulta correcta, no caducada, con saldo)
2. Urgencia (si incluso la más urgente supera `min_urgency_hours`, 72 h por defecto → no cambiar)
3. Ya es el objetivo → no cambiar
4. Enfriamiento (`cooldown_minutes`, 120 por defecto)
5. Protección de actividad (conversación en los últimos `active_guard_minutes`, 30 por defecto → no cambiar)
6. Filtro de valor (saldo por debajo de `min_remaining_credits` → no vale la pena)
7. Antioscilación (diferencia menor que `min_gap_hours`, 24 por defecto → no cambiar)

**Límite de efecto**: solo cambia la cuenta por defecto de sesiones futuras; la sesión en curso no se cambia en caliente — recárgala por ACP o reinicia el CLI.

</details>

<details>
<summary><b>Capacidades de la aplicación (compartidas por los seis ámbitos)</b></summary>

| Capacidad | Descripción |
| --- | --- |
| Ajustes de la app (⚙) | Tema (claro / oscuro / sistema), idioma de la interfaz (简体中文 / English), inicio silencioso en la bandeja, cerrar a la bandeja, actualización automática |
| Copia y restauración globales | Cubren todos los ámbitos a la vez, seleccionables por cuentas / configuración / pasarela, **sin credenciales por defecto** |
| Gestor de Skills | Analiza `skills/*/SKILL.md`, activa/desactiva cada entrada (reescritura línea a línea, resto de bytes intacto), migra entre regiones e importa desde zip |
| Alojamiento de ajustes del cliente | Lee y escribe el `settings.json` del cliente de origen: tabla de seis ranuras, huella de formato, parche mínimo a nivel de byte, copia / restauración / diferencias |
| Ventana de sesiones (solo escritorio) | Ventana pequeña con estados de tareas de varias fuentes; solo lectura, clic para volver a la sesión original |
| Vigilancia de sesiones del cliente (apagada por defecto) | Instala un hook en el `settings.json` de WorkBuddy y envía los estados a la ventana (receptor `127.0.0.1:57893`) |

</details>

## Datos y privacidad

- Todo vive en `~/.buddy-switch/` en esta máquina y no se envía a terceros; las preferencias de interfaz también son locales.
- Los almacenes de cuentas están separados por ámbito (p. ej. `accounts.json` / `accounts.global.json`, `trae/checkin_accounts*.json`).
- Las copias exportadas **no incluyen tokens por defecto**; el texto plano exige marcar una casilla y confirmar dos veces, con aviso de que el archivo permite iniciar sesión directamente.
- Las claves de pasarela solo guardan hash y prefijo; **los datos locales no se versionan** y antes de publicar se escanean patrones de tokens.

## Desarrollo

```mermaid
flowchart TB
    FE["src/ · React 19 + Vite<br/>6 destinos · api.ts doble canal"] --> H["Anfitriones: comandos Tauri | HTTP (axum)"]
    H --> CORE["crates/buddy-switch-core<br/>cuentas · auth · registros · sesiones · ámbitos"]
    H --> GW["crates/buddy-switch-gateway<br/>WB / Trae / Qoder / unificada"]
    CORE --> DATA[("~/.buddy-switch/")]
    GW --> DATA
```

```bash
npm install
npm run tauri dev     # desarrollo de escritorio
npm run build         # build del frontend + controles de contrato / ámbitos / plataformas
npm run check:api     # solo comprobación del contrato de API
```

```
crates/
  buddy-switch-core/     # cuentas, auth, OAuth, procesos, cambio, sesiones, registros, regiones, módulos Trae / Qoder
  buddy-switch-gateway/  # pasarelas compatibles con OpenAI / Anthropic (cuatro implementaciones)
  buddy-switch-server/   # servidor HTTP + CLI (API axum + frontend incrustado con rust-embed)
src-tauri/               # anfitrión de escritorio (envoltorios finos de Tauri + bandeja + anfitriones de pasarela)
src/                     # frontend: components / pages / lib / locales
npm/                     # paquetes npm (sin publicar)
```

Requisitos de entorno, proceso de publicación y convenciones de directorios: [docs/guides/DEVELOPMENT.md](docs/guides/DEVELOPMENT.md).
**Mapa completo de la documentación: [docs/README.md](docs/README.md)** — guías / especificaciones vigentes / histórico de solo lectura / prototipo.

## Contribuir

**Este proyecto ha crecido con contribuciones**: la plataforma Qoder, la pasarela unificada, el desfase aleatorio de las programaciones y los README multilingües salieron de issues y pull requests. No hace falta ser experto.

| | |
| --- | --- |
| 🐞 **Reportar un fallo** | Con la versión y una captura del registro: [abrir una issue](https://github.com/NextAgentX/qoder-trae-workbuddy-switch/issues/new) |
| 🧩 **Añadir una plataforma** | Las diferencias se declaran una vez en `src/lib/platforms/registry.ts` y `product.rs`; las páginas no se tocan. |
| 🌍 **Corregir traducciones** | Los textos van por parejas `src/locales/domains/*.zh.ts` ↔ `*.en.ts`; también se agradecen revisiones de los ocho README. |
| 📸 **Actualizar capturas** | Un comando rehace todas las imágenes claro/oscuro: ver la cabecera de `scripts/gen-readme-shots.mjs`. |
| 🧪 **Añadir un test** | La mayoría de los tests de guarda los añadieron colaboradores; empieza por la lista de controles en `docs/guides/DEVELOPMENT.md`. |

Guía completa (qué se puede hacer, checklist de PR, convenciones de código): **[CONTRIBUTING.en.md](CONTRIBUTING.en.md)** ([简体中文](CONTRIBUTING.md)).

## Apoyar el proyecto

- ⭐ **Dar una estrella** — la ayuda más directa: el proyecto se encuentra antes y el autor sabe que merece la pena mantenerlo.
- 🐞 **Describir bien un problema** — una issue reproducible vale por diez «no funciona» y suele entrar en la siguiente versión.
- ☕ **Invitar un café al autor** (totalmente opcional, ninguna función queda bloqueada)

<table>
  <thead>
    <tr><th>WeChat Pay</th><th>Alipay</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/donate-wechat.png" alt="WeChat Pay QR" width="240" /></td>
      <td><img src="docs/images/donate-alipay.jpg" alt="Alipay QR" width="240" /></td>
    </tr>
  </tbody>
</table>

## Agradecimientos

- [changexbc/workbuddy-switch](https://github.com/changexbc/workbuddy-switch) — por el formato de datos de cuenta de WorkBuddy y el enfoque de cambio multicuenta.
- [Sliverkiss/workbuddy2api](https://github.com/Sliverkiss/workbuddy2api) — por la idea de exponer la cuota de modelos como servicio compatible con OpenAI.

Cada proyecto sigue su propia licencia. Este proyecto es una implementación independiente, sin afiliación ni respaldo; los nombres y marcas pertenecen a sus titulares.

## Aviso legal

Texto completo en **[docs/policies/DISCLAIMER.en.md](docs/policies/DISCLAIMER.en.md)** ([简体中文](docs/policies/DISCLAIMER.md)), seis apartados: herramienta de terceros no oficial; términos de servicio y cumplimiento; escritura de datos y copias; riesgos de la automatización; riesgos de exponer la pasarela API; provista «tal cual». Usar el proyecto implica aceptarlos; si no, deja de usarlo y desinstálalo.

## Licencia

**[PolyForm Noncommercial License 1.0.0](./LICENSE)**: se permite el uso personal no comercial, **el uso comercial no está licenciado**.

- Texto completo: [LICENSE](./LICENSE)
- Explicación sencilla y preguntas frecuentes: [docs/policies/LICENSING.en.md](docs/policies/LICENSING.en.md) ([简体中文](docs/policies/LICENSING.md))
- Para licencia comercial, contacta al autor mediante los Issues del repositorio.

## Historial de estrellas

<a href="https://www.star-history.com/?repos=NextAgentX%2Fqoder-trae-workbuddy-switch&type=date&legend=top-left">

 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=NextAgentX/qoder-trae-workbuddy-switch&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=NextAgentX/qoder-trae-workbuddy-switch&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=NextAgentX/qoder-trae-workbuddy-switch&type=date&legend=top-left" />
 </picture>
</a>
