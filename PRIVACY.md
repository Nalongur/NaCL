# NaCL 隐私说明 / Privacy Notice

生效日期 / Effective date: 2026-07-31

本说明适用于 Na Craft Launcher（NaCL）当前公开开发版本。NaCL 仍处于 Pre-alpha 阶段；Microsoft 登录和完整游戏启动流程尚未发布。

This notice applies to the current public development version of Na Craft Launcher (NaCL). NaCL is still in Pre-alpha; Microsoft sign-in and the complete game launch flow have not been released.

## 中文

### 1. 本地处理的数据

NaCL 为实现启动器功能，会在用户的 Windows 设备上读取或保存以下数据：

- 启动器设置、实例元数据与实例游戏目录，默认位于 `%APPDATA%\NaCL`
- 版本清单缓存、游戏资源、依赖库、下载文件、托管 Java 运行环境与本地日志，默认位于 `%LOCALAPPDATA%\NaCL`
- 用户主动选择的 Java 路径，以及用于推荐内存分配的系统物理内存信息
- 用户主动创建的离线档案名称；离线档案不会验证 Minecraft 所有权

这些数据用于本地功能，不会由 NaCL 自动上传到项目维护者控制的服务器。

### 2. 网络请求

在用户刷新版本、安装实例或下载 Java 时，NaCL 可能直接连接：

- Mojang/Minecraft 官方服务，用于获取版本清单、版本元数据、游戏客户端、资源文件和依赖库
- Eclipse Adoptium API 及其下载地址，用于查询和下载 Eclipse Temurin Java 运行环境

这些服务可能按照各自的隐私政策处理 IP 地址、请求时间和常规网络元数据。NaCL 当前没有自建账号服务器、遥测服务器或广告服务。

### 3. Microsoft 登录

Microsoft 登录目前尚未开放。正式实现时，NaCL 将遵循以下原则：

- 通过 Microsoft 官方 OAuth 授权流程登录，不在 NaCL 内要求或收集 Microsoft 密码
- 仅在用户主动登录、验证 Minecraft Java Edition 资格或启动游戏时使用授权令牌
- 短期访问令牌仅在必要期间使用；刷新令牌计划使用 Windows 用户级加密保护
- 不向与登录和 Minecraft 服务无关的第三方出售或共享账号令牌

在功能正式实现前，本节描述的是设计承诺，而不是当前已启用的功能。

### 4. 遥测、日志与诊断

当前版本不包含使用分析、广告追踪、崩溃自动上报或后台遥测。诊断日志保存在本地，除非用户自行选择并提交，否则不会发送给项目维护者。

通过 GitHub Issue 寻求帮助时，请勿公开粘贴访问令牌、刷新令牌、完整系统用户名或其他敏感信息。

### 5. 数据删除与保留

用户可以在退出 NaCL 后删除 `%APPDATA%\NaCL` 和 `%LOCALAPPDATA%\NaCL` 中的数据。删除实例目录可能同时删除游戏存档、截图和本地配置，请先自行备份重要内容。

NaCL 项目维护者不保存上述本地数据，因此无法代替用户恢复或删除设备上的文件。

### 6. 变更与联系

功能或数据处理方式发生变化时，本说明会随代码仓库更新。隐私问题可以通过本仓库的 [GitHub Issues](https://github.com/Nalongur/NaCL/issues) 提出；请不要在公开 Issue 中提交敏感数据。

## English

### 1. Data processed locally

NaCL reads or stores the following data on the user's Windows device to provide launcher functionality:

- Launcher settings, instance metadata, and instance game directories under `%APPDATA%\NaCL` by default
- Version manifest caches, game assets, libraries, downloads, managed Java runtimes, and local logs under `%LOCALAPPDATA%\NaCL` by default
- A Java path selected by the user and physical memory information used to recommend memory allocation
- Offline profile names created by the user; offline profiles do not verify Minecraft ownership

This data is used for local functionality and is not automatically uploaded to servers controlled by the NaCL maintainers.

### 2. Network requests

When the user refreshes versions, installs an instance, or downloads Java, NaCL may connect directly to:

- Official Mojang/Minecraft services for version manifests, version metadata, the game client, assets, and libraries
- The Eclipse Adoptium API and its download locations to query and download Eclipse Temurin Java runtimes

Those services may process IP addresses, request times, and ordinary network metadata under their own privacy policies. NaCL currently has no maintainer-operated account server, telemetry server, or advertising service.

### 3. Microsoft sign-in

Microsoft sign-in is not currently available. When implemented, NaCL will follow these principles:

- Use Microsoft's official OAuth authorization flow and never request or collect a Microsoft password inside NaCL
- Use authorization tokens only when the user initiates sign-in, Minecraft: Java Edition entitlement verification, or game launch
- Use short-lived access tokens only as needed; refresh tokens are planned to be protected with Windows user-scoped encryption
- Never sell or share account tokens with third parties unrelated to authentication or Minecraft services

Until the feature is implemented, this section describes design commitments rather than an active feature.

### 4. Telemetry, logs, and diagnostics

The current version contains no usage analytics, advertising trackers, automatic crash reporting, or background telemetry. Diagnostic logs remain local unless the user chooses to submit them.

When requesting support through a GitHub Issue, do not publicly post access tokens, refresh tokens, full operating-system usernames, or other sensitive information.

### 5. Data deletion and retention

After closing NaCL, users may delete data under `%APPDATA%\NaCL` and `%LOCALAPPDATA%\NaCL`. Deleting an instance directory may also delete worlds, screenshots, and local configuration, so important files should be backed up first.

The NaCL maintainers do not retain this local data and therefore cannot restore or delete files on the user's device.

### 6. Changes and contact

This notice will be updated in the repository when features or data-handling practices change. Privacy questions may be submitted through this repository's [GitHub Issues](https://github.com/Nalongur/NaCL/issues). Do not include sensitive data in a public issue.
