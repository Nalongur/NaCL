# Na Craft Launcher

NaCL 是一款面向 Windows 的极简 Minecraft: Java Edition 启动器，使用 Tauri、Vue、TypeScript 与 Rust 构建。

NaCL is a minimalist Minecraft: Java Edition launcher for Windows, built with Tauri, Vue, TypeScript, and Rust.

> [!IMPORTANT]
> **开发状态 / Development status: Pre-alpha**
>
> 当前仓库用于公开开发和技术预览。Microsoft 登录、离线档案与完整游戏启动流程尚未完成，也没有面向普通玩家发布的稳定安装包。请勿将当前源码构建视为可替代官方启动器的日常版本。
>
> This repository is for public development and technical preview. Microsoft sign-in, offline profiles, and the complete game launch flow are not finished. No stable end-user installer is available. Do not treat current source builds as a daily-use replacement for the official launcher.

> [!NOTE]
> **NaCL 不是 Minecraft 官方产品，未经 Mojang 或 Microsoft 批准，也不与其存在关联。**
>
> **NOT AN OFFICIAL MINECRAFT PRODUCT. NOT APPROVED BY OR ASSOCIATED WITH MOJANG OR MICROSOFT.**

![NaCL 深色首页 / NaCL dark home page](design/renders/nacl-app-home-dark.png)

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
| 原版实例安装 | 部分完成 | 可下载版本元数据、客户端、依赖库和资源文件，并写入隔离实例目录；完整启动参数生成和进程启动尚未完成 |
| 实例管理 | 部分完成 | 支持创建、配置、复制、删除和打开实例目录；实例修复、导入导出和运行状态管理仍待完善 |
| Java 管理 | 部分完成 | 支持检测本机 Java、手动选择可执行文件和下载托管 Eclipse Temurin；版本与游戏版本的最终匹配策略仍待完善 |
| 内存设置 | 可用 | 检测设备物理内存，并为实例提供受约束的内存分配滑块 |
| 下载策略 | 可用 | 支持并发数、重试、超时和限速设置，并强制执行下载后完整性校验 |
| 日志与诊断 | 部分完成 | 支持本地日志查看、删除、保留期清理、目录打开和存储占用诊断；尚无自动崩溃上报 |
| Microsoft 登录 | 未实现 | Entra 应用已准备申请资格；待 Minecraft Java Edition Game Service API 获批后接入官方 OAuth 流程 |
| 离线档案 | 未实现 | 将只创建本地身份，不验证 Minecraft 所有权；当前尚未接入最终启动流程 |
| 游戏启动 | 未实现 | Java 进程、完整 classpath、JVM/游戏参数、原生库和认证参数尚未组合为正式启动管线 |
| Forge、Fabric 与模组 | 暂不纳入 | 不属于当前原版首版范围；实例数据结构与页面布局为后续扩展保留空间 |

当前适合测试界面交互、版本目录、原版文件安装、Java 管理、实例配置和本地诊断。不适合使用真实 Microsoft 账号启动游戏，也不提供可下载的稳定发行版。

计划中的开发顺序：

1. 完成 Microsoft API 资格申请和官方登录流程
2. 完成 Minecraft 资格校验、档案读取与安全令牌存储
3. 实现原版游戏启动参数生成、原生库处理和进程生命周期管理
4. 完善失败恢复、文件修复、日志脱敏和首个可测试安装包
5. 在原版启动稳定后，再讨论 Forge、Fabric 与模组管理

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
| Vanilla instance installation | Partial | Downloads version metadata, the client, libraries, and assets into isolated instance storage; final launch arguments and process startup are not implemented |
| Instance management | Partial | Creates, configures, copies, deletes, and opens instance directories; repair, import/export, and running-state management remain unfinished |
| Java management | Partial | Detects local Java installations, supports manual executable selection, and downloads managed Eclipse Temurin runtimes; final game-to-Java compatibility policy remains unfinished |
| Memory settings | Available | Detects physical memory and exposes a constrained per-instance allocation slider |
| Download behavior | Available | Configures concurrency, retries, timeout, and bandwidth limits, with enforced post-download integrity verification |
| Logs and diagnostics | Partial | Lists, reads, deletes, and expires local logs and reports storage use; automatic crash reporting is not included |
| Microsoft sign-in | Not implemented | The Entra application is prepared for review; official OAuth integration depends on Minecraft Java Edition Game Service API approval |
| Offline profiles | Not implemented | These will create local identities only and will not verify Minecraft ownership; they are not connected to the final launch flow |
| Game launch | Not implemented | Java process creation, the full classpath, JVM/game arguments, native libraries, and authentication arguments are not yet assembled into a launch pipeline |
| Forge, Fabric, and mods | Out of current scope | Excluded from the first vanilla-focused release; the instance model and layout retain room for later extension |

The current build is suitable for testing UI behavior, version discovery, vanilla file installation, Java management, instance settings, and local diagnostics. It is not suitable for launching the game with a real Microsoft account, and no stable downloadable release is provided.

Planned development order:

1. Complete the Microsoft API eligibility process and official sign-in flow
2. Add Minecraft entitlement checks, profile retrieval, and protected token storage
3. Implement vanilla launch arguments, native-library handling, and process lifecycle management
4. Improve recovery, file repair, log redaction, and produce the first testable installer
5. Discuss Forge, Fabric, and mod management after the vanilla launch path is stable

See the bilingual [Privacy Notice / 隐私说明](PRIVACY.md) for local data, network requests, third-party services, and the planned Microsoft authentication flow.

## 技术栈 / Technology

- [Tauri 2](https://tauri.app/)：Windows 桌面壳与 Rust 命令桥接 / Windows desktop shell and Rust command bridge
- [Vue 3](https://vuejs.org/)：界面与交互状态 / interface and interaction state
- TypeScript：前端类型与组件逻辑 / frontend types and component logic
- Rust：实例、下载、Java、存储和诊断核心 / instance, download, Java, storage, and diagnostics core

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

构建成功只代表源码通过对应检查，不代表 Microsoft 登录、完整游戏启动或真实账号环境已经验证。

A successful build only confirms the corresponding source checks. It does not verify Microsoft sign-in, complete game launch, or a live account environment.

## 本地数据 / Local data

用户设置和实例配置默认保存在 `%APPDATA%\NaCL`；可重新下载的版本数据、Java 运行环境、临时下载和日志默认保存在 `%LOCALAPPDATA%\NaCL`。构建产物不会提交到仓库。

User settings and instance configuration are stored under `%APPDATA%\NaCL` by default. Re-downloadable version data, managed Java runtimes, temporary downloads, and logs are stored under `%LOCALAPPDATA%\NaCL` by default. Build artifacts are not committed.

未来的 Microsoft 登录不会在 NaCL 内收集密码。短期访问令牌只用于用户主动发起的身份验证、Minecraft 资格校验和启动流程；刷新令牌计划使用 Windows 用户级加密保护。完整的数据清单、联网目标与删除方法见 [`PRIVACY.md`](PRIVACY.md)。

The planned Microsoft sign-in flow will never collect a Microsoft password inside NaCL. Short-lived access tokens will only support user-initiated authentication, Minecraft entitlement checks, and launch. Refresh tokens are planned to use Windows user-scoped protection. See [`PRIVACY.md`](PRIVACY.md) for the full data inventory, network destinations, and deletion instructions.

## 字体 / Typeface

界面标题使用 [得意黑 Smiley Sans](https://github.com/atelier-anchor/smiley-sans)。字体由 atelierAnchor 发布，采用 SIL Open Font License 1.1；随项目分发的字体文件及完整许可见 [`app/src/assets/fonts/OFL.txt`](app/src/assets/fonts/OFL.txt)。

Interface headings use [Smiley Sans](https://github.com/atelier-anchor/smiley-sans) by atelierAnchor under the SIL Open Font License 1.1. The bundled font and full license text are available at [`app/src/assets/fonts/OFL.txt`](app/src/assets/fonts/OFL.txt).

## 声明 / Disclaimer

NaCL 是独立社区项目，不隶属于 Mojang Studios、Microsoft 或 Xbox，也未获得其批准或背书。Minecraft 名称及相关资产归各自权利人所有。使用本项目时仍需遵守 Minecraft EULA、Microsoft 服务条款与 [Minecraft Usage Guidelines](https://www.minecraft.net/en-us/usage-guidelines)。

NaCL is an independent community project. It is not affiliated with, approved by, or endorsed by Mojang Studios, Microsoft, or Xbox. The Minecraft name and related assets belong to their respective owners. Use of this project remains subject to the Minecraft EULA, Microsoft terms, and the [Minecraft Usage Guidelines](https://www.minecraft.net/en-us/usage-guidelines).

项目代码当前未单独授予开源许可证。公开可见不等于允许复制、修改或重新分发；仓库内的第三方字体继续遵循其自身许可证。

The project code does not currently have a separate open-source license. Public visibility does not grant permission to copy, modify, or redistribute it. Third-party fonts remain governed by their own licenses.
