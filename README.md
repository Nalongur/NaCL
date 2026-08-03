# Na Craft Launcher

NaCL 是一款面向 Windows 的极简 Minecraft: Java Edition 启动器，使用 Tauri、Vue、TypeScript 与 Rust 构建。

NaCL is a minimalist Minecraft: Java Edition launcher for Windows, built with Tauri, Vue, TypeScript, and Rust.

版本变更见[更新日志](CHANGELOG.md)。See the [changelog](CHANGELOG.md) for release notes.

> [!IMPORTANT]
> **第一阶段正式版 / Stage 1 Release: NaCL 0.3.0**
>
> NaCL 0.3.0 是项目的第一个公开阶段版本，提供 Windows 便携程序与安装包。原版安装、离线启动、Microsoft 正版登录与在线启动已经接入，并完成了一次真实账号、官方皮肤/披风资料和单人世界启动回归验证。文件修复、完整异常恢复、代码签名和广泛版本兼容验证仍未完成；重要存档仍建议使用官方启动器并自行备份。
>
> NaCL 0.3.0 is the project's first public stage release, with a Windows portable build and installers. Vanilla installation, offline launch, Microsoft sign-in, authenticated launch, official skin/cape profile loading, and a real single-player launch regression have been verified once. File repair, complete failure recovery, code signing, and broad version compatibility testing remain unfinished; use the official launcher and keep backups for important worlds.

> [!NOTE]
> **NaCL 不是 Minecraft 官方产品，未经 Mojang 或 Microsoft 批准，也不与其存在关联。**
>
> **NOT AN OFFICIAL MINECRAFT PRODUCT. NOT APPROVED BY OR ASSOCIATED WITH MOJANG OR MICROSOFT.**

![NaCL 深色首页 / NaCL dark home page](design/renders/nacl-app-home-dark.png)

## 下载 / Download

请从 [GitHub Releases](https://github.com/Nalongur/NaCL/releases) 获取 0.3.0。首个 Release 提供便携版、NSIS 安装程序与 MSI 安装程序；当前构建尚未进行代码签名，Windows 可能显示 SmartScreen 提示。

Download 0.3.0 from [GitHub Releases](https://github.com/Nalongur/NaCL/releases). The first release includes a portable executable, an NSIS installer, and an MSI installer. The binaries are not code-signed yet, so Windows may display a SmartScreen warning.

## 项目状态 / Project status

### 中文

状态含义：

- **可用**：核心路径已经接入界面并具备对应后端实现
- **部分完成**：可以测试主要流程，但尚未达到完整启动器标准
- **未实现**：界面可能已经预留，实际业务流程尚未接入
- **暂不纳入**：不属于当前原版启动器阶段

| 模块 | 状态 | 当前能力与边界 |
| --- | --- | --- |
| 桌面界面 | 可用 | 双层侧栏、深浅主题、极简平面图标、低开销页面切换动画和右侧滑出面板已接入 |
| 原版版本目录 | 可用 | 可从 Mojang 官方版本清单读取正式版与快照版信息，并支持搜索和筛选 |
| 原版实例安装 | 部分完成 | 可下载版本元数据、客户端、依赖库和资源文件，并写入隔离实例目录；实例修复和断点恢复仍待完善 |
| 实例管理 | 部分完成 | 支持创建、配置、复制、删除和打开实例目录；实例修复、导入导出和运行状态管理仍待完善 |
| Java 管理 | 部分完成 | 支持检测本机 Java、手动选择可执行文件和下载托管 Eclipse Temurin；版本与游戏版本的最终匹配策略仍待完善 |
| 内存设置 | 可用 | 检测设备物理内存，并为实例提供受约束的内存分配滑块 |
| 下载策略 | 可用 | 使用 Rust 原生 Range 分片与流式回退引擎，支持断点状态、多连接、重试、限速、实时速度和下载后完整性校验；不依赖外置 aria2 |
| 日志与诊断 | 部分完成 | 支持本地日志查看、删除、保留期清理、目录打开和存储占用诊断；尚无自动崩溃上报 |
| 存储路径 | 可用 | 实例安装目录与共享缓存目录可独立选择并恢复默认；切换时不会自动移动或删除旧数据 |
| Microsoft 登录 | 部分完成 | 使用系统浏览器、授权码与 PKCE 完成 Microsoft、Xbox、XSTS 和 Minecraft Services 认证；刷新令牌由 Windows 当前用户级 DPAPI 加密保存 |
| 离线档案 | 可用 | 可创建和修改仅保存在本机的用户名与稳定 UUID；不验证 Minecraft 所有权，也不能替代 Microsoft 登录 |
| 游戏启动 | 部分完成 | 支持离线身份与 Microsoft 正版身份启动，向游戏传入官方玩家名、UUID 和短期访问令牌，使游戏读取账号当前皮肤与披风；尚缺文件修复和完整崩溃归因 |
| Forge、Fabric 与模组 | 暂不纳入 | 不属于当前原版首版范围；实例数据结构与页面布局为后续扩展保留空间 |

0.3.0 是第一个可公开下载的阶段版本，适合体验界面交互、原版文件安装、离线与 Microsoft 正版启动、Java 管理、自定义存储路径、实例配置和本地诊断。它仍是早期版本，不代表已经达到完整启动器的长期稳定标准。

计划中的开发顺序：

1. 扩大 Microsoft 正版登录、在线启动与原版版本兼容回归
2. 完善启动失败恢复、文件修复、崩溃归因和日志脱敏
3. 增加代码签名、安装升级与发布自动化
4. 在原版启动稳定后，再讨论 Forge、Fabric 与模组管理

### English

Status definitions:

- **Available**: the primary path is connected to the UI and backed by an implementation
- **Partial**: the main flow can be tested but is not yet complete enough for a production launcher
- **Not implemented**: UI space may exist, but the underlying workflow is not connected
- **Out of current scope**: intentionally excluded from the current vanilla-launcher phase

| Area | Status | Current capability and boundary |
| --- | --- | --- |
| Desktop interface | Available | Dual-layer navigation, dark and light themes, flat icons, lightweight page transitions, and right-side drawers are connected |
| Vanilla version catalog | Available | Reads release and snapshot metadata from Mojang's official version manifest, with search and filtering |
| Vanilla instance installation | Partial | Downloads version metadata, the client, libraries, and assets into isolated instance storage; repair and complete resume behavior remain unfinished |
| Instance management | Partial | Creates, configures, copies, deletes, and opens instance directories; repair, import/export, and running-state management remain unfinished |
| Java management | Partial | Detects local Java installations, supports manual executable selection, and downloads managed Eclipse Temurin runtimes; final game-to-Java compatibility policy remains unfinished |
| Memory settings | Available | Detects physical memory and exposes a constrained per-instance allocation slider |
| Download behavior | Available | Uses a native Rust Range-segmented downloader with resumable state and streaming fallback, including multiple connections, retries, bandwidth limits, live throughput, and integrity verification; no external aria2 executable is required |
| Logs and diagnostics | Partial | Lists, reads, deletes, and expires local logs and reports storage use; automatic crash reporting is not included |
| Storage paths | Available | Instance installation and shared-cache directories can be selected independently and reset to defaults; switching does not automatically move or delete old data |
| Microsoft sign-in | Partial | Uses the system browser with authorization code and PKCE, then completes Microsoft, Xbox, XSTS, and Minecraft Services authentication; the refresh token is protected with Windows current-user DPAPI |
| Offline profiles | Available | Creates and edits a local-only username and stable UUID; it does not verify Minecraft ownership or replace Microsoft sign-in |
| Game launch | Partial | Launches with either an offline identity or an authenticated Microsoft identity and supplies the official name, UUID, and short-lived token so the game can load the account's active skin and cape; repair and complete crash attribution remain unfinished |
| Forge, Fabric, and mods | Out of current scope | Excluded from the first vanilla-focused release; the instance model and layout retain room for later extension |

NaCL 0.3.0 is the first publicly downloadable stage release. It is suitable for trying the interface, vanilla installation, offline and Microsoft-authenticated launch, Java management, custom storage paths, instance settings, and local diagnostics. It remains an early release and does not yet meet the long-term stability standard of a complete launcher.

Planned development order:

1. Expand Microsoft sign-in, authenticated launch, and vanilla-version regression coverage
2. Improve launch recovery, file repair, crash attribution, and log redaction
3. Add code signing, installer upgrades, and release automation
4. Discuss Forge, Fabric, and mod management after the vanilla launch path is stable

See the bilingual [Privacy Notice / 隐私说明](PRIVACY.md) for local data, network requests, third-party services, and Microsoft authentication handling.

## 技术栈 / Technology

- [Tauri 2](https://tauri.app/)：Windows 桌面壳与 Rust 命令桥接 / Windows desktop shell and Rust command bridge
- [Vue 3](https://vuejs.org/)：界面与交互状态 / interface and interaction state
- TypeScript：前端类型与组件逻辑 / frontend types and component logic
- Rust：实例、下载、Java、存储和诊断核心 / instance, download, Java, storage, and diagnostics core

## 下载实现 / Download implementation

NaCL 使用项目内的 Rust 原生下载器，根据服务器 Range 能力和文件大小在分片与流式模式之间选择。构建和运行均不需要 `aria2c.exe`。

NaCL uses its in-project native Rust downloader and selects segmented or streaming mode according to server Range support and file size. Neither building nor running NaCL requires `aria2c.exe`.

## 目录结构 / Repository layout

```text
NaCL/
├─ app/
│  ├─ crates/launcher-core/   # 与界面无关的 Rust 核心 / UI-independent Rust core
│  ├─ src/                    # Vue 界面 / Vue interface
│  └─ src-tauri/              # Tauri 桌面壳 / Tauri desktop shell
├─ design/                    # 设计过程、SVG 与渲染图 / design studies, SVGs, and renders
├─ PRIVACY.md                 # 中英文隐私说明 / bilingual privacy notice
└─ README.md
```

用户界面使用的得意黑字体保留在 `app/src/assets/fonts/`，设计目录中的中间方案不参与应用运行。

The Smiley Sans font used by the interface remains under `app/src/assets/fonts/`. Intermediate studies under `design/` are not required at runtime.

## Windows 开发环境 / Windows development

需要准备 / Requirements:

- Windows 10/11
- Node.js 20 或更高版本 / Node.js 20 or later
- Rust stable 与 MSVC 工具链 / Rust stable and the MSVC toolchain
- Microsoft Edge WebView2 Runtime

启动开发版本 / Start the development build:

```powershell
cd app
npm.cmd install
npm.cmd run tauri dev
```

构建无安装包的调试程序 / Build a debug executable without bundling:

```powershell
cd app
npm.cmd run tauri build -- --debug --no-bundle
```

检查前端类型并构建静态资源 / Type-check and build the frontend:

```powershell
cd app
npm.cmd run build
```

运行 Rust 工作区测试 / Run Rust workspace tests:

```powershell
cd app
cargo test --workspace
```

构建成功只代表源码通过对应检查，不代表 Microsoft 登录、所有 Minecraft 版本或真实设备上的图形启动已经验证。

A successful build only confirms the corresponding source checks. It does not verify Microsoft sign-in, every Minecraft version, or graphics startup on a live device.

## 本地数据 / Local data

用户设置默认保存在 `%APPDATA%\NaCL`；实例安装目录与共享缓存目录可在设置页独立更改。Java 运行环境和日志仍默认保存在 `%LOCALAPPDATA%\NaCL`。切换路径不会自动搬移旧数据，构建产物不会提交到仓库。

User settings remain under `%APPDATA%\NaCL` by default. Instance installation and shared-cache directories can be changed independently in Settings. Managed Java and logs remain under `%LOCALAPPDATA%\NaCL` by default. Switching paths does not automatically move old data, and build artifacts are not committed.

Microsoft 登录始终在系统浏览器的微软官方页面进行，NaCL 不收集密码。短期访问令牌只用于身份验证、Minecraft 资格校验、档案读取和用户主动启动；刷新令牌使用 Windows 当前用户级 DPAPI 加密，并在退出账号时删除。完整的数据清单、联网目标与删除方法见 [`PRIVACY.md`](PRIVACY.md)。

Microsoft sign-in always occurs on Microsoft's official page in the system browser; NaCL does not collect passwords. Short-lived tokens are used only for authentication, entitlement checks, profile retrieval, and user-initiated launch. The refresh token is encrypted with Windows current-user DPAPI and deleted on sign-out. See [`PRIVACY.md`](PRIVACY.md) for the full data inventory, network destinations, and deletion instructions.

## 字体 / Typeface

界面标题使用 [得意黑 Smiley Sans](https://github.com/atelier-anchor/smiley-sans)。字体由 atelierAnchor 发布，采用 SIL Open Font License 1.1；随项目分发的字体文件及完整许可见 [`app/src/assets/fonts/OFL.txt`](app/src/assets/fonts/OFL.txt)。

Interface headings use [Smiley Sans](https://github.com/atelier-anchor/smiley-sans) by atelierAnchor under the SIL Open Font License 1.1. The bundled font and full license text are available at [`app/src/assets/fonts/OFL.txt`](app/src/assets/fonts/OFL.txt).

## 声明 / Disclaimer

NaCL 是独立社区项目，不隶属于 Mojang Studios、Microsoft 或 Xbox，也未获得其批准或背书。Minecraft 名称及相关资产归各自权利人所有。使用本项目时仍需遵守 Minecraft EULA、Microsoft 服务条款与 [Minecraft Usage Guidelines](https://www.minecraft.net/en-us/usage-guidelines)。

NaCL is an independent community project. It is not affiliated with, approved by, or endorsed by Mojang Studios, Microsoft, or Xbox. The Minecraft name and related assets belong to their respective owners. Use of this project remains subject to the Minecraft EULA, Microsoft terms, and the [Minecraft Usage Guidelines](https://www.minecraft.net/en-us/usage-guidelines).

项目代码当前未单独授予开源许可证。公开可见不等于允许复制、修改或重新分发；仓库内的第三方字体继续遵循其自身许可证。

The project code does not currently have a separate open-source license. Public visibility does not grant permission to copy, modify, or redistribute it. Third-party fonts remain governed by their own licenses.
