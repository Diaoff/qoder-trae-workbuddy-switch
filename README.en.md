<p align="center">
  <img src="public/icon-transparent.png" alt="Buddy Switch icon" width="120" />
</p>

<p align="center">
  <strong>Buddy Switch</strong><br />
  A multi-account workbench for WorkBuddy · CodeBuddy · TraeWork · Qoder
</p>

<p align="center">
  <a href="README.md">简体中文</a> ·
  <strong>English</strong> ·
  <a href="README.ja.md">日本語</a> ·
  <a href="README.ko.md">한국어</a> ·
  <a href="README.ru.md">Русский</a> ·
  <a href="README.fr.md">Français</a> ·
  <a href="README.es.md">Español</a> ·
  <a href="README.pt.md">Português</a>
</p>

# Buddy Switch

A **local-first** multi-account workbench: it gathers accounts from four product lines — **WorkBuddy / CodeBuddy / TraeWork / Qoder** — and **6 scopes** into one place. QR / device-code sign-in, one-click login-state switching, check-in and credits, usage analytics, session-replica syncing — plus exposing your model quota to other local AI tools through OpenAI / Anthropic compatible endpoints.

> ⚠️ **Unofficial third-party tool** (source-available, PolyForm Noncommercial). Not affiliated with, authorized or endorsed by those products or their rights holders. It reads and writes authentication data of local clients, sends automated requests on the schedule you configure, and can forward your quota through a local port. Read the **[Disclaimer](docs/DISCLAIMER.en.md)** before use.

## Screenshots

> Taken from a demo run (`VITE_DEMO_MODE=1`, stubbed request layer). Accounts, credits and request records are fictional.

<table>
  <thead>
    <tr><th>Overview (light)</th><th>Overview (dark)</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/overview-light.png" alt="Overview, light" /></td>
      <td><img src="docs/images/overview-dark.png" alt="Overview, dark" /></td>
    </tr>
  </tbody>
</table>

<table>
  <thead>
    <tr><th>WorkBuddy accounts (light)</th><th>WorkBuddy accounts (dark)</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/workbuddy-accounts-light.png" alt="WorkBuddy accounts, light" /></td>
      <td><img src="docs/images/workbuddy-accounts-dark.png" alt="WorkBuddy accounts, dark" /></td>
    </tr>
  </tbody>
</table>

<table>
  <thead>
    <tr><th>Usage · trend (light)</th><th>Usage · trend (dark)</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/usage-trend-light.png" alt="Usage trend, light" /></td>
      <td><img src="docs/images/usage-trend-dark.png" alt="Usage trend, dark" /></td>
    </tr>
  </tbody>
</table>

<table>
  <thead>
    <tr><th>Gateway (light)</th><th>Gateway (dark)</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/api-service-light.png" alt="Gateway, light" /></td>
      <td><img src="docs/images/api-service-dark.png" alt="Gateway, dark" /></td>
    </tr>
  </tbody>
</table>

More pages (sessions / settings / TraeWork accounts and usage): [docs/images](docs/images).

## The big picture

```mermaid
flowchart TB
    subgraph SRC["3 product lines · 6 scopes"]
        direction LR
        WB["WorkBuddy<br/>CN / Global"]
        TR["TraeWork<br/>CN / Global"]
        QD["Qoder<br/>CN / Global"]
    end
    subgraph CORE["Local core · ~/.buddy-switch/"]
        direction LR
        AC["Account stores<br/>credentials · recycle bin"]
        AU["Automation<br/>check-in · keepalive · rotation"]
        US["Usage<br/>tokens / credits"]
        SE["Session replicas<br/>content fingerprint · additive sync"]
        GW["Quota gateway<br/>OpenAI / Anthropic compatible"]
    end
    WB --> AC
    TR --> AC
    QD --> AC
    AC --- AU --- US --- SE
    US --> GW
    GW --> OUT["Other local AI tools<br/>Cursor · Claude Code · OpenWebUI"]
```

**Three hard rules** hold everywhere: data stays on this machine, credentials are only written into client login state (no proxy login, no upstream protocol hacks), and every write is backed up first.

## Three forms

| Form | How to get it | Notes |
| --- | --- | --- |
| **Desktop app** | Download from [GitHub Releases](https://github.com/NextAgentX/trae-workbuddy-switch/releases/latest) | Packaged with Tauri; recommended for daily use |
| **webui** | Build from source (below) | Same frontend bundle as the desktop app, served over local HTTP |
| **Online demo** | [GitHub Pages](https://nextagentx.github.io/trae-workbuddy-switch/) | Read-only demo; accounts and usage are fictional |

> The npm package is not published yet, so `npm i -g` will not install it; use the source build below for the webui form.

## Quick start

**Desktop app**: download the installer for your platform from Releases.

| Platform | Installer | Install |
| --- | --- | --- |
| macOS Apple Silicon (arm64) | `BuddySwitch_<version>_aarch64.dmg` | Open the DMG, drag `BuddySwitch.app` into Applications |
| macOS Intel (x86_64) | `BuddySwitch_<version>_x86_64.dmg` | Same as above |
| Windows x64 | `BuddySwitch_<version>_x64-setup.exe` | Run the installer |
| Linux x64 | `BuddySwitch_<version>_amd64.deb` / `.AppImage` | Install the `.deb`, or make the AppImage executable and run it |

**webui (build from source)**: the frontend bundle is embedded into the server **at compile time** (`rust-embed`), so the order matters.

```bash
npm install
npm run build                                    # 1) frontend → dist/
cargo build --release -p buddy-switch-server     # 2) build the server (embeds dist/)
./target/release/buddy-switch                    # 3) serve + open browser (127.0.0.1:57890)
```

```bash
buddy-switch              # serve + open browser
buddy-switch serve        # serve only (--port to override)
buddy-switch status       # print the current account in the terminal
buddy-switch version      # version
```

<details>
<summary><b>macOS: "cannot verify developer" / "no permission" when switching</b></summary>

- Cannot verify developer: in Finder, Control-click the app → Open, or go to System Settings → Privacy & Security → "Open Anyway".
- Switching accounts writes the client's auth file: grant BuddySwitch **App Management** (or **Full Disk Access**) in System Settings → Privacy & Security, then restart the app. The in-app "permission check" verifies it any time.
- In webui form, permissions follow the terminal process that started the service.

</details>

## 6 scopes (platform × region)

The scope selector at the top-left decides which set of accounts you are managing. The scope lives in the URL (`/s/wb-cn/accounts`), so refresh, sharing and back/forward all stay put; `⌘K` / `Ctrl+K` searches across scopes, `⌘1`–`⌘6` switches directly.

| Scope | Platform | Region | Clients managed |
| --- | --- | --- | --- |
| `wb-cn` | WorkBuddy | CN | WorkBuddy desktop, CodeBuddy CN IDE / CLI, CodeBuddy plugins for VS Code / JetBrains |
| `wb-global` | WorkBuddy | Global | Same clients (global builds) |
| `trae-cn` | TraeWork | CN | Trae CN |
| `trae-global` | TraeWork | Global | Trae |
| `qoder-cn` | Qoder | CN | Qoder CN desktop / CLI |
| `qoder-global` | Qoder | Global | Qoder desktop / CLI |

The six scopes are **fully independent**: separate account stores, separate clients, separate config, separate gateway keys. Trae's two legacy program lines (TraeWork / TraeCode) were merged upstream into one client; the old ids remain for compatibility only.

## 6 sidebar destinations

Identical item for item across all three platforms: **Overview · Accounts · Usage · Sessions · Gateway · Settings**. Entries are gated by **capability** (not by platform branch): no capability, no entry — and a direct URL still yields a structured empty state.

| Destination | In one line |
| --- | --- |
| **Overview** | Things needing attention (re-login / today's check-in / expiring credits / client not running) + stat cards + 14-day token chart + a glance at other scopes |
| **Accounts** | Card and table views, program slots, QR or device-code sign-in, local import, check-in, recycle bin (30 days), client rate-limit ledger (WorkBuddy only) |
| **Usage** | One workbench, four views: trend · models · sessions · credits; a single parameter bar (range + data source) |
| **Sessions** | Replicas of the same session across accounts: who lags, who diverged, sync to whom (**WorkBuddy only**) |
| **Gateway** | Independent gateways (one per scope) and the unified gateway (one provider across scopes) |
| **Settings** | Nine sections: general · account & login · check-in & credits · gateway · schedules · client · permissions & diagnostics · logs · backup & restore |

### Capability matrix

| Capability | WorkBuddy | TraeWork | Qoder |
| --- | :---: | :---: | :---: |
| Account store / sign-in / local import | ✅ QR + import | ✅ Web login + JWT | ✅ Device code + PAT |
| Program slots | ✅ 5 (4 switchable + 1 read-only probe) | ✅ 1 (merged client) | ✅ 2 (desktop / CLI) |
| Check-in / credits | ✅ incl. auto-travel, auto-rotation | ✅ | ✅ daily check-in + plan / quota |
| Usage analytics | ✅ tokens + credits | ✅ tokens + credits | ⏳ data source pending |
| Session replica sync | ✅ content fingerprint + additive | — | — |
| Quota gateway | ✅ `:57891` | ✅ `:57894` | ✅ `:57895` |

## How switching an account works

```mermaid
flowchart LR
    A["Pick an account"] --> B["Back up current login state"] --> C["Quit the client"] --> D["Write target account"] --> E["Verify and restart"]
    D -->|"optional"| F["Also: copy sessions<br/>merge memory and connectors"]
```

- **Backup before write**: auth files, Trae login snapshots and session overwrites all leave a rollback copy first.
- **Verify after write**: a failed write is reverted automatically; a running client is refused, so it cannot write its in-memory state back over yours.
- **Delete is soft**: accounts go to the recycle bin for 30 days and can be restored; permanent deletion also removes credentials and snapshots.

## Model quota gateway

Expose the current account's quota to local tools: OpenAI compatible (`/v1/models`, `/v1/chat/completions`, `/v1/responses`) and Anthropic compatible (`/v1/messages`), both with SSE streaming. Loopback-only by default.

```mermaid
flowchart LR
    C["Local AI tools"] -->|"sk-wb-…"| G1[":57891<br/>WorkBuddy gateway"]
    C -->|"sk-trae-…"| G2[":57894<br/>Trae gateway"]
    C -->|"sk-qoder-…"| G3[":57895<br/>Qoder gateway"]
    C -->|"sk-uni-…"| G4[":57892<br/>Unified gateway"]
    G1 --> P1["wb-cn / wb-global pools"]
    G2 --> P2["trae-cn / trae-global pools"]
    G3 --> P3["qoder-cn / qoder-global pools"]
    G4 -->|"model name carries a namespace prefix"| P1
    G4 --> P2
    G4 --> P3
```

- **Four independent listeners**: ports form one **`5789x` block** — `57890` UI · `57891` WorkBuddy · `57892` unified · `57893` session hook · `57894` Trae · `57895` Qoder. Turning the unified gateway off affects no existing client; the `/v1/*` routes overlap verbatim, so they **cannot** be merged into one Router.
- **Key prefixes are one family**: `sk-wb-` · `sk-trae-` · **`sk-qoder-`** · `sk-uni-` — the prefix alone tells you which port to configure.
- **Key lifecycle**: revoke · restore · delete (only revoked keys can be deleted). The plaintext is returned once at creation; the store keeps sha256 plus the first/last 4 characters.
- **Account pool**: picks accounts by balance and rate-limit state, can exclude unusable ones, probes "account × model" limits, and revives them with one click.

The **unified gateway** solves "the same model name overwriting itself across platforms": model names become `<namespace>:<model-id>`, where namespaces derive from the scope registry (currently six: `wb-cn` / `wb-global` / `trae-cn` / `trae-global` / `qoder-cn` / `qoder-global`).

| Rule | Behaviour |
| --- | --- |
| Bare model name (no prefix) | **400** with the list of valid prefixes (no guessing a default and silently hitting the wrong pool) |
| Namespace parsing | **Whole-segment exact match only** (`trae` / `traework` / `traecode` are prefixes of one another) |
| One key, many platforms | A single `sk-uni-` key can authorize several platforms at once |
| `/v1/responses` on `trae-*` | **501** — never a silent downgrade to chat |
| Default state | **Off**; enable it in the Gateway page's "Unified gateway" type |

> Architecture: [UNIFIED-GATEWAY-ARCH.md](docs/redesign/UNIFIED-GATEWAY-ARCH.md) · Client snippets: [GATEWAY-CLIENT-INTEGRATION.md](docs/GATEWAY-CLIENT-INTEGRATION.md)

## Automation

Schedules support **minute-level** times (e.g. `09:37`), multiple times per task, and **random jitter** (±5/15/30 minutes) so runs scatter instead of firing on the hour.

| Side | Tasks |
| --- | --- |
| WorkBuddy (6) | Check-in · cat travel · activity map · token keepalive · back-to-school · night owl |
| Trae (2) | Check-in · automatic JWT renewal |

<details>
<summary><b>Auto-rotation policy (WorkBuddy, prevents credit expiry)</b></summary>

A background loop sets the account used by **subsequent CodeBuddy CLI launches** to the most urgent one (earliest expiry with credits left). Decision order:

1. Valid account (query ok, not expired, credits left)
2. Urgency (if even the most urgent has more than `min_urgency_hours` left, default 72h → no switch)
3. Already the target → no switch
4. Cooldown (`cooldown_minutes`, default 120)
5. Active guard (conversation within the last `active_guard_minutes`, default 30 → no switch)
6. Value filter (remaining credits below `min_remaining_credits` → not worth it)
7. Anti-jitter (gap smaller than `min_gap_hours`, default 24 → no switch)

**Boundary**: only the default account for future sessions changes; the running session is not hot-swapped — reload via ACP or restart the CLI.

</details>

<details>
<summary><b>App-level capabilities (shared by all six scopes)</b></summary>

| Capability | Notes |
| --- | --- |
| App settings (⚙) | Theme (light / dark / system), UI language (简体中文 / English), silent start to tray, close-to-tray, auto-update |
| Global backup & restore | Covers every scope at once, selectable by accounts / config / gateway, **no credentials by default** |
| Skill manager | Scans `skills/*/SKILL.md`, toggles each entry (line-level rewrite, other bytes preserved), moves entries across regions, imports from zip |
| Client settings hosting | Reads and writes the upstream client's own `settings.json`: six-slot spec table, format fingerprint, minimal byte-level patch, backup / restore / diff |
| Session overlay (desktop only) | A small always-on-top window with cross-source task states; read-only, click to focus the original session |
| Client session watching (off by default) | Installs a hook into WorkBuddy's `settings.json` and streams task states into the overlay (receiver `127.0.0.1:57893`) |

</details>

## Data and privacy

- Everything lives in `~/.buddy-switch/` on this machine; nothing is uploaded. UI preferences stay local too.
- Account stores are per scope (e.g. `accounts.json` / `accounts.global.json`, `trae/checkin_accounts*.json`).
- Exported backups contain **no tokens by default**; plaintext tokens require an explicit checkbox plus a second confirmation, with a warning that the file can be used to sign in directly.
- Gateway keys store only a hash and a prefix; **local data is never committed**, and token patterns are scanned before release.

## Development

```mermaid
flowchart TB
    FE["src/ · React 19 + Vite<br/>6 destinations · api.ts dual channel"] --> H["Hosts: Tauri commands | HTTP (axum)"]
    H --> CORE["crates/buddy-switch-core<br/>accounts · auth · check-in · sessions · scopes"]
    H --> GW["crates/buddy-switch-gateway<br/>WB / Trae / Qoder / unified"]
    CORE --> DATA[("~/.buddy-switch/")]
    GW --> DATA
```

```bash
npm install
npm run tauri dev     # desktop development
npm run build         # frontend build + contract / scope / platform gates
npm run check:api     # API contract check only
```

```
crates/
  buddy-switch-core/     # accounts, auth, OAuth, processes, switching, sessions, check-in, regions, Trae / Qoder modules
  buddy-switch-gateway/  # OpenAI / Anthropic compatible gateways (four implementations)
  buddy-switch-server/   # HTTP server + CLI (axum API + rust-embed frontend)
src-tauri/               # desktop host (thin Tauri wrappers + tray + gateway hosts)
src/                     # frontend: components / pages / lib / locales
npm/                     # npm packages (not published yet)
```

Environment requirements, release process and layout conventions: [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## Contributing

Issues, documentation, bug fixes and feature PRs are all welcome — see **[CONTRIBUTING.en.md](CONTRIBUTING.en.md)** ([简体中文](CONTRIBUTING.md)) for the checklist and conventions.

## Support the project

<table>
  <thead>
    <tr><th>WeChat Pay</th><th>Alipay</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/donate-wechat.png" alt="WeChat Pay QR code" width="240" /></td>
      <td><img src="docs/images/donate-alipay.jpg" alt="Alipay QR code" width="240" /></td>
    </tr>
  </tbody>
</table>

## Acknowledgements

- [changexbc/workbuddy-switch](https://github.com/changexbc/workbuddy-switch) — for the WorkBuddy account data format and the multi-account switching approach.
- [Sliverkiss/workbuddy2api](https://github.com/Sliverkiss/workbuddy2api) — for the idea of exposing model quota as an OpenAI-compatible service.

Each project follows its own license. This project is an independent implementation with no affiliation, partnership or endorsement relationship with them; all names and trademarks belong to their respective owners.

## Disclaimer

Full text in **[docs/DISCLAIMER.en.md](docs/DISCLAIMER.en.md)** ([简体中文](docs/DISCLAIMER.md)), six sections: unofficial third-party tool; terms of service and compliance; data writes and backups; risks of automated behaviour; API gateway exposure; provided "as is". Using the project means you have read and agree to them; if not, stop using it and uninstall.

## License

**[PolyForm Noncommercial License 1.0.0](./LICENSE)**: noncommercial use is permitted, **commercial use is not licensed**.

- Full text: [LICENSE](./LICENSE)
- Plain-language summary and FAQ: [docs/LICENSING.en.md](docs/LICENSING.en.md) ([简体中文](docs/LICENSING.md))
- For commercial licensing, contact the author via this repository's Issues.

## Star history

<a href="https://www.star-history.com/?repos=NextAgentX%2Ftrae-workbuddy-switch&type=date&legend=top-left">

 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=NextAgentX/trae-workbuddy-switch&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=NextAgentX/trae-workbuddy-switch&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=NextAgentX/trae-workbuddy-switch&type=date&legend=top-left" />
 </picture>
</a>
