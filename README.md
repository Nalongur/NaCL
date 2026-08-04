# Na Craft Launcher

NaCL 是一款面向 Windows 的极简 Minecraft: Java Edition 启动器，使用 Tauri、Vue、TypeScript 与 Rust 构建。

NaCL is a minimalist Minecraft: Java Edition launcher for Windows, built with Tauri, Vue, TypeScript, and Rust.

> [!IMPORTANT]
> **NaCL 不是 Minecraft 官方产品，未经 Mojang 或 Microsoft 批准，也不与其存在关联。**
>
> **NOT AN OFFICIAL MINECRAFT PRODUCT. NOT APPROVED BY OR ASSOCIATED WITH MOJANG OR MICROSOFT.**

![NaCL 深色首页 / NaCL dark home page](design/renders/nacl-app-home-dark.png)

## 功能 / Features

- **多加载器实例**：安装并管理原版、Fabric、Quilt、Forge 与 NeoForge 隔离实例。
- **社区内容管理**：从 Modrinth 搜索、安装、更新、启停和移除 Mod、资源包、光影包与数据包，也可导入本地文件。
- **整合包安装**：支持在线 Modrinth 项目和本地 `.mrpack`，校验文件大小、SHA-1/SHA-512 与归档路径。
- **依赖与兼容诊断**：读取常见 Mod 元数据，检查缺少前置、重复顶层 ID 和加载器不匹配；正确识别 Fabric JAR-in-JAR 模块。
- **实例维护**：创建、复制、删除、配置和导出实例，校验并补全受管文件，分析常见崩溃原因。
- **Microsoft 与离线身份**：支持系统浏览器 Microsoft 登录、正版启动、官方皮肤/披风资料，以及仅保存在本机的离线档案。
- **Java 与内存管理**：检测本机 Java、选择自定义运行时、下载托管 Eclipse Temurin，并按实例配置 JVM 与内存。
- **原生下载引擎**：Rust Range 分片下载、断点状态、重试、限速、流式回退和完整性校验，不依赖外置 aria2。
- **本地存储与日志**：实例目录和共享缓存可独立配置；日志、诊断和崩溃分析默认仅在本机处理。
- **桌面体验**：深浅主题、平面图标、双层导航、右侧任务抽屉，以及加载完成后切换到主窗口的独立启动标识。

- **Multi-loader instances** for Vanilla, Fabric, Quilt, Forge, and NeoForge.
- **Modrinth content management** for mods, resource packs, shaders, data packs, and `.mrpack` modpacks.
- **Dependency and compatibility diagnostics**, including Fabric nested-module detection.
- **Instance maintenance**, managed-file verification/repair, crash analysis, and privacy-conscious ZIP export.
- **Microsoft-authenticated and offline launch**, Java/runtime management, and per-instance JVM settings.
- **Native Rust downloading** with segmented Range requests, resume state, retries, limits, fallback, and integrity checks.

## 下载 / Download

从 [GitHub Releases](https://github.com/Nalongur/NaCL/releases) 下载最新 Windows x64 便携版、EXE 安装程序或 MSI 安装程序。

Download the latest Windows x64 portable build, EXE installer, or MSI installer from [GitHub Releases](https://github.com/Nalongur/NaCL/releases).

当前发布包尚未进行代码签名，Windows 可能显示 SmartScreen 提示。Current binaries are not code-signed and may trigger a Windows SmartScreen warning.

## 当前边界 / Current boundaries

- NaCL 仍处于早期阶段；已完成定向真实启动验证，但尚未覆盖所有 Minecraft、Java、加载器和第三方整合包组合。
- 修改实例目录或共享缓存目录不会自动迁移、合并或删除旧数据。
- 离线档案不验证 Minecraft 所有权，也不能替代 Microsoft 登录。
- 重要存档请自行备份；构建成功不等于所有设备和图形环境均已验证。
- NaCL remains an early-stage launcher. Targeted live launches have passed, but broad compatibility coverage is still incomplete.

版本变化见[更新日志](CHANGELOG.md)。本地数据、联网目标、身份验证和删除方式见[隐私说明](PRIVACY.md)。

See the [changelog](CHANGELOG.md) for release details and the [privacy notice](PRIVACY.md) for local data, network destinations, authentication, and deletion behavior.

## 技术栈 / Technology

- Tauri 2：Windows 桌面壳与 Rust 命令桥接
- Vue 3 + TypeScript：界面与交互状态
- Rust：实例、加载器、下载、Java、启动、内容和维护核心

## 开发 / Development

要求 Windows 10/11、Node.js 20+、Rust stable/MSVC 和 Microsoft Edge WebView2 Runtime。

```powershell
cd app
npm.cmd install
npm.cmd run tauri dev
```

生产构建：

```powershell
cd app
npm.cmd run tauri -- build
```

## 声明 / Disclaimer

NaCL 是独立社区项目，不隶属于 Mojang Studios、Microsoft 或 Xbox，也未获得其批准或背书。Minecraft 名称及相关资产归各自权利人所有。

NaCL is an independent community project and is not affiliated with, approved by, or endorsed by Mojang Studios, Microsoft, or Xbox. Minecraft names and related assets belong to their respective owners.

项目代码当前未单独授予开源许可证。仓库内的第三方字体继续遵循其自身许可证；得意黑许可见 [`app/src/assets/fonts/OFL.txt`](app/src/assets/fonts/OFL.txt)。
