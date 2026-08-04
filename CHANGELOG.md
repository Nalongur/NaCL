# 更新日志 / Changelog

本项目遵循[语义化版本](https://semver.org/lang/zh-CN/)。

This project follows [Semantic Versioning](https://semver.org/).

## [0.4.0] - 2026-08-04

NaCL 0.4.0 将模组加载器、社区内容与 Modrinth 整合包三阶段合并为首个模组支持版本，继续保留现有 NaCL 实例、下载队列和右侧抽屉交互。

NaCL 0.4.0 combines loader installation, community content management, and Modrinth modpack installation into the first mod-capable release while retaining NaCL's existing instance, queue, and drawer interactions.

### 新增 / Added

- 支持 Fabric、Quilt、Forge 与 NeoForge 版本目录、安装配置和独立实例启动元数据。
- 在实例“模组与资源”页搜索、安装、更新、启停和可恢复移除 Mod、资源包与光影包，并支持本地文件导入。
- 增加批量更新、批量启停与批量移除；读取 Fabric、Quilt、Forge 和 NeoForge 本地 Mod 元数据，并诊断重复 ID、缺少前置与加载器不匹配。
- 数据包按具体 Minecraft 存档搜索、安装、导入和管理，避免写入无效的实例全局目录。
- 实例“文件”页新增大小/SHA-1 校验与补全、最近崩溃日志本地分析，以及排除日志和运行缓存的 ZIP 导出。
- 支持在线 Modrinth 项目与本地 `.mrpack` 创建新实例，自动读取 Minecraft/加载器依赖并应用客户端覆盖文件。
- 新增独立启动标识窗口，主 WebView 完成关键加载后才显示，避免启动阶段出现黑白空窗。
- Added Fabric, Quilt, Forge, and NeoForge instance profiles; Modrinth content browsing and management; local content import; and online/local `.mrpack` installation.
- Added batch content updates/toggling/removal, local Fabric/Quilt/Forge/NeoForge metadata diagnostics, per-world data-pack management, managed-file verification and repair, local crash analysis, and privacy-conscious instance ZIP export.
- Added a dedicated startup-mark window so the main WebView remains hidden until critical data is ready instead of exposing a blank native frame.

### 校验与安全 / Validation and safety

- 下载内容优先校验 SHA-512，回退到 SHA-1；整合包还会校验声明大小、归档格式、路径穿越和符号链接。
- 加载器配置迁移和内容清单使用原子写入；内容操作按实例互斥，整合包失败会回滚新实例。
- Fabric JAR-in-JAR 模块仅用于满足依赖，不再作为顶层 Mod ID 展示或误报重复；递归读取受路径、层级、数量和解压大小限制。
- Downloaded content is verified with SHA-512 or SHA-1. Archive paths and links are constrained, manifests are atomically replaced, content tasks are serialized per instance, and failed modpack installs remove the newly created instance.
- Fabric JAR-in-JAR modules satisfy dependencies without being displayed or diagnosed as duplicate top-level IDs; recursive inspection is bounded by path, depth, count, and extracted-size limits.

### 当前验证边界 / Current validation boundary

- 自动化测试与生产构建已覆盖数据迁移、路径校验、加载器元数据合并和界面类型检查。
- 已完成一次定向真实回归：Fabric 0.19.3 / Minecraft 26.1.1（含 Fabric API）、Quilt 0.20.0-beta.9 / 1.21.1、Forge 52.1.16 / 1.21.1 与 NeoForge 21.1.248 / 1.21.1 均进入渲染和声音初始化阶段。
- 已用 Modrinth `RnOABzNk` 验证在线 `.mrpack`：10 个远程文件共 14,176,983 字节全部通过 SHA-1/SHA-512 与大小复核，覆盖文件落地后实例加载 96 个模组。
- 以上加载器回归使用离线档案；更多 Minecraft/加载器组合、正版账号模组启动、光影实际渲染和大型第三方整合包仍需扩展验证。Forge/NeoForge 官方安装器步骤在 NaCL 隔离缓存中运行。
- Automated tests and production builds cover migrations, path checks, loader metadata merging, and UI typing. Targeted live launches reached renderer and sound initialization for Fabric 0.19.3 / Minecraft 26.1.1 with Fabric API, Quilt 0.20.0-beta.9 / 1.21.1, Forge 52.1.16 / 1.21.1, and NeoForge 21.1.248 / 1.21.1.
- The Modrinth `RnOABzNk` `.mrpack` was installed online: all 10 remote files (14,176,983 bytes) matched their declared size, SHA-1, and SHA-512 values, overrides were applied, and the resulting instance loaded 96 mods. These loader checks used an offline profile; broader version combinations, authenticated modded launches, actual shader rendering, and large third-party packs remain pending.

## [0.3.0] - 2026-08-03

NaCL 0.3.0 是项目的第一个公开阶段正式版。本版本提供 Windows 便携程序、NSIS 与 MSI 安装程序，并完成一次 Microsoft 正版账号、官方资料和原版单人世界启动回归验证。

NaCL 0.3.0 is the project's first public Stage 1 release. It ships as a Windows portable executable plus NSIS and MSI installers, with one completed regression of Microsoft authentication, official profile data, and vanilla single-player launch.

### 新增 / Added

- 接入 Microsoft 系统浏览器登录、PKCE、Xbox/XSTS、Minecraft 资格与官方档案读取。
- 使用 Windows 当前用户级 DPAPI 加密保存刷新令牌；短期令牌仅保存在内存中。
- 正版启动传入官方玩家名、UUID 与 Minecraft 访问令牌，使游戏加载账号当前皮肤和披风。
- 首页和侧栏显示官方皮肤头像、正版状态与当前启用披风，并保留离线档案切换。
- 实例安装目录与共享缓存目录可独立自定义和恢复默认，不自动移动或删除旧数据。
- Added system-browser Microsoft sign-in with PKCE, Xbox/XSTS exchange, Minecraft entitlement validation, official profile data, DPAPI-protected refresh tokens, authenticated launch, skin/cape status, and independently configurable instance/cache paths.

### 修复与改进 / Fixed and Changed

- 下载系统改为 Rust 原生 Range 分片与流式回退，不再包含或调用外置 aria2。
- 安装锁在异常退出时可靠释放，Java 下载接入统一进度与取消控制。
- 保持设置页第二栏选择、第三栏内容切换的原有交互；存储路径功能直接加入“存储与维护”。
- Replaced external aria2 with the native downloader, hardened install-state cleanup, unified Java download controls, and retained the existing second/third-column settings interaction.

### 已知限制 / Known limitations

- 当前仅面向 Windows 与 Minecraft: Java Edition 原版实例，不包含 Forge、Fabric 或模组管理。
- 文件修复、完整断点恢复、崩溃归因与广泛版本兼容回归仍在完善。
- 更改实例或缓存路径不会自动迁移、合并或删除旧目录。
- 首个发布包尚未进行代码签名，Windows 可能显示 SmartScreen 提示。
- Windows and vanilla Minecraft: Java Edition only; Forge, Fabric, and mod management are not included.
- File repair, complete resume recovery, crash attribution, and broad version regression remain in progress.
- Changing instance or cache paths does not migrate, merge, or delete previous directories.
- The first release binaries are not code-signed and may trigger a Windows SmartScreen warning.

## [0.2.0] - 2026-07-31

### 新增 / Added

- 新增本地离线档案，使用用户名生成稳定 UUID，并支持直接启动已安装的原版 Minecraft 实例。
- 根据官方版本元数据生成 classpath、JVM 参数和游戏参数，解压 Windows 原生库并选择兼容的 Java 运行环境。
- 新增游戏进程日志记录、日志列表查看和启动失败诊断；首次启动失败时也会保留可见日志。
- 新增游戏运行状态与退出检测，启动器可识别游戏结束并恢复可启动状态。
- 安装队列新增实时进度、速度、暂停、取消及“查看详情”入口。
- Added local offline profiles with stable UUIDs and direct launch for installed vanilla Minecraft instances.
- Added metadata-driven classpath, JVM/game arguments, Windows native extraction, compatible Java selection, game logs, and process lifecycle tracking.

### 修复与改进 / Fixed and Changed

- Windows 启动游戏时改用 `javaw.exe`，避免额外显示 Java 控制台窗口。
- 下载中的右侧安装面板可随时收起到安装队列，任务会继续运行；左侧下载分类栏保持固定。
- 修复日志详情关闭按钮偏移、首次启动失败无日志，以及游戏关闭后启动器无法识别的问题。
- 首页选择版本不再自动跳转到实例页。
- 版本设置中的游戏版本、Java 与 JVM、内存、显示、文件和高级选项分别打开对应设置内容。
- 更新中英文项目说明和隐私说明，使其与离线档案、离线启动及本地游戏日志行为一致。
- Switched Windows launches to `javaw.exe`, improved log and exit detection, kept downloads active when the install drawer is collapsed, and separated instance-setting panels by function.
