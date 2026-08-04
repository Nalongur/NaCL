# NaCL 隐私说明 / Privacy Notice

生效日期 / Effective date: 2026-08-03

适用版本 / Applies to: NaCL 0.3.0, NaCL 0.4.0, source builds, and locally built development versions

本说明用于公开 Na Craft Launcher（NaCL）当前及计划中的数据处理边界。NaCL 是本地桌面应用，目前没有由项目维护者运营的账号、同步、遥测、广告或日志收集服务器。

This notice describes the current and planned data-handling boundaries of Na Craft Launcher (NaCL). NaCL is a local desktop application. The maintainers currently operate no account, synchronization, telemetry, advertising, or log-collection server.

> [!IMPORTANT]
> NaCL 0.3.0 已实现 Microsoft 正版登录。登录在系统浏览器完成；刷新令牌使用 Windows 当前用户级 DPAPI 加密，短期访问令牌不写入普通配置或日志。
>
> NaCL 0.3.0 implements Microsoft sign-in. Authentication occurs in the system browser; the refresh token is encrypted with Windows current-user DPAPI, while short-lived access tokens are not written to ordinary configuration or logs.

## 中文

### 1. 适用范围与角色

本说明适用于从本仓库构建的 NaCL 桌面应用，不替代 Microsoft、Mojang、Minecraft、Eclipse Foundation、GitHub 或下载托管方各自的隐私政策。

在当前版本中：

- NaCL 维护者不接收应用内账号注册信息，因为项目没有自建账号系统
- NaCL 维护者不自动接收本地设置、实例、日志、Java 路径或诊断信息
- 当用户主动访问第三方服务时，相应服务提供方会按照其自身政策处理网络请求
- 用户主动在 GitHub 提交 Issue、日志或截图时，GitHub 和项目维护者会收到用户选择公开的内容

### 2. 本地数据清单

| 数据类别 | 具体内容 | 用途 | 默认位置 | 保留方式 |
| --- | --- | --- | --- | --- |
| 启动器设置 | 主题、默认实例、Java 选择、内存和界面相关设置 | 恢复用户偏好 | `%APPDATA%\NaCL\config` | 保留到用户修改或删除 |
| 下载设置 | 并发数、重试次数、超时、限速和校验设置 | 控制下载行为 | `%APPDATA%\NaCL\config` | 保留到用户修改或删除 |
| 存储路径设置 | 用户选择的实例安装目录和共享缓存目录 | 将实例与可重新下载缓存保存到指定位置 | `%APPDATA%\NaCL\config\storage-paths.json` | 保留到恢复默认或删除 |
| 离线档案 | 用户输入的游戏用户名和据此生成的稳定 UUID | 本地离线身份与游戏启动 | `%APPDATA%\NaCL\config\offline-profile.json` | 保留到用户修改或删除 |
| Microsoft 账号档案 | Minecraft UUID、玩家名、官方皮肤与披风的公开资料 URL 和状态 | 显示账号并准备正版启动 | `%APPDATA%\NaCL\config\microsoft-account.json` | 保留到退出账号或手动删除 |
| Microsoft 刷新令牌 | Microsoft 返回的长期刷新令牌，经 Windows 当前用户级 DPAPI 加密 | 应用重启后恢复登录并获取新的短期令牌 | `%APPDATA%\NaCL\config\microsoft-refresh-token.bin` | 退出账号时删除；也可手动删除 |
| 实例元数据 | 实例名称、Minecraft 版本、内存、显示和高级参数 | 管理隔离实例 | 默认 `%APPDATA%\NaCL\instances\<instance-id>`，可自定义 | 保留到用户删除实例 |
| 实例游戏目录 | 存档、截图、配置及游戏生成的数据 | 提供实例隔离 | 实例目录下的 `<instance-id>\game` | 保留到用户删除或迁移 |
| 模组与内容清单 | 加载器版本、Mod/资源包/光影包文件、来源版本、哈希、启用状态和整合包索引 | 安装、校验、更新和管理模组内容 | 实例目录及其 `.nacl` 子目录 | 保留到用户移除内容或删除实例；移除文件先进入实例回收目录 |
| 版本与资源缓存 | 版本清单、版本 JSON、客户端、依赖库、资源索引和资源对象 | 安装与校验原版文件 | 默认 `%LOCALAPPDATA%\NaCL\cache`，可自定义 | 保留到用户清理或删除 |
| 托管 Java | 下载的 Eclipse Temurin JRE 及运行时信息 | 为对应游戏版本提供 Java | `%LOCALAPPDATA%\NaCL\runtimes` | 保留到用户清理或删除 |
| 临时下载 | 下载中的 `.part` 文件及安装中间文件 | 支持下载重试和原子替换 | `%LOCALAPPDATA%\NaCL\cache\downloads` | 完成后移除；失败文件可由用户清理 |
| 本地日志 | 启动器运行、下载、安装、游戏标准输出和错误诊断文本 | 用户本地排错 | `%LOCALAPPDATA%\NaCL\logs` | 按用户设置的保留期或手动删除 |
| 系统诊断 | 操作系统类型、CPU 架构、物理内存和目录占用等运行环境信息 | Java 选择、内存建议和本地诊断 | 运行时读取；报告默认不上传 | 仅在功能需要时读取 |
| 手动选择的文件路径 | Java 可执行文件路径、用户主动打开或选择的目录 | 使用指定运行环境或打开本地位置 | 写入对应本地设置时保留 | 保留到用户修改或删除 |

NaCL 不会主动扫描用户的文档、浏览器数据、密码库、通讯录或与启动器无关的文件。实例目录中的 Minecraft 存档和截图只作为本地文件存在，当前不会自动上传。

### 3. 网络请求与第三方服务

联网操作由用户触发的版本刷新、实例安装、Java 下载、Microsoft 登录和游戏启动产生。离线启动不向 NaCL 维护者发送档案，但 Minecraft 客户端本身仍可能访问 Mojang/Microsoft 服务或用户选择的服务器。

| 服务或目标 | 触发条件 | 发送或暴露的数据 | 返回内容 |
| --- | --- | --- | --- |
| Mojang 版本元数据服务（`piston-meta.mojang.com`） | 刷新版本目录或读取版本元数据 | 常规 HTTPS 请求信息，例如 IP 地址、请求时间、客户端网络信息 | 版本清单和版本 JSON |
| Minecraft 资源服务（`resources.download.minecraft.net`） | 安装或修复原版资源 | 常规 HTTPS 请求信息和所请求资源的哈希路径 | Minecraft 资源对象 |
| 版本 JSON 指定的官方文件地址 | 安装原版客户端和依赖库 | 常规 HTTPS 请求信息及所请求文件路径 | 客户端、库文件、原生库和资源索引 |
| Eclipse Adoptium API（`api.adoptium.net`）及其下载地址 | 用户选择下载托管 Java | Windows 平台、CPU 架构、所需 Java 主版本和常规 HTTPS 请求信息 | Eclipse Temurin JRE 元数据与压缩包 |
| Microsoft、Xbox 与 Minecraft 身份服务 | 用户主动选择 Microsoft 登录、恢复登录、资格校验或正版启动 | OAuth 授权请求、短期令牌、Xbox/XSTS 交换材料及服务要求的账号信息 | Minecraft 短期访问令牌、资格、UUID、玩家名、皮肤与披风档案 |
| GitHub | 用户访问仓库、提交 Issue 或主动上传诊断材料 | 用户自行提交的账号信息、Issue 文本、附件及 GitHub 记录的网络元数据 | 项目源码、Issue 与支持沟通 |
| Modrinth API 与其 CDN | 用户搜索、安装或更新社区内容与整合包 | 搜索词、Minecraft/加载器版本、所请求项目或文件及常规 HTTPS 网络信息 | 项目元数据、版本清单、Mod、资源包、光影包和 `.mrpack` 文件 |

NaCL 不会把 Microsoft 令牌、离线档案或本地日志发送到 Eclipse Adoptium；也不会把 Java 下载信息发送给 Mojang。各服务只接收完成对应请求所需的信息。

第三方可能根据其政策处理 IP 地址、设备与浏览器信息、请求时间、Cookie 或账号数据。请参阅：

- [Microsoft Privacy Statement](https://www.microsoft.com/privacy/privacystatement)
- [Eclipse Foundation Website Privacy Policy](https://www.eclipse.org/legal/privacy/)
- [GitHub Privacy Statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement)

### 4. Microsoft 登录与令牌设计

NaCL 的 Microsoft 登录遵循以下实现边界：

1. 登录界面由 Microsoft 官方授权页面提供；NaCL 不嵌入仿制密码框，也不要求用户向 NaCL 输入 Microsoft 密码
2. 使用适合 Windows 公共桌面客户端的 OAuth 授权流程，并采用防止授权码被截获或重放的保护措施
3. 只请求完成登录、Xbox/Minecraft 身份交换、Minecraft 所有权校验、档案读取和用户主动启动游戏所需的权限
4. 短期访问令牌只在必要期间使用，避免写入普通设置文件或日志
5. 刷新令牌使用 Windows 当前用户范围的 DPAPI 加密保护，并与普通实例配置分开保存；Minecraft、Xbox 和 Microsoft 短期访问令牌只保存在运行内存中
6. 注销账号时删除 NaCL 保存的本地令牌材料；用户仍可在 Microsoft 账号页面撤销应用授权
7. 不出售令牌，不用于广告画像，也不交给与身份验证和 Minecraft 服务无关的第三方
8. 日志和错误信息必须在写入前移除授权标头、完整令牌和其他可直接登录的机密

为启动 Minecraft，短期 Minecraft 访问令牌必须作为官方版本定义的启动参数传给本机 Java 进程，因此在游戏运行期间可能被具有足够本机权限的进程检查工具看到。这是本地启动协议的一部分；NaCL 不把该参数写入自己的日志。实现可能随 Microsoft 或 Minecraft API 要求变化，并同步更新本说明。

### 5. 离线档案

离线档案在本机创建显示名称和稳定 UUID，用于不需要官方在线验证的原版游戏启动。离线档案：

- 不会验证用户是否拥有 Minecraft
- 不会绕过需要 Microsoft/Xbox/Minecraft 在线认证的服务
- 不会自动上传到 NaCL 维护者服务器
- 可能与在线服务器的 UUID、皮肤、权限或白名单规则不兼容

NaCL 将离线用户名、UUID 和空的在线认证字段传给本机 Minecraft 进程，不会把离线档案上传到项目维护者运营的服务。Minecraft 客户端或用户加入的服务器仍可能按其自身规则处理用户名和 UUID。

### 6. 遥测、日志与支持材料

当前版本不包含：

- 使用分析或行为追踪 SDK
- 广告标识符或广告网络
- 自动崩溃上报
- 后台遥测或远程配置
- 维护者运营的日志上传服务

本地日志可能包含版本号、文件路径、Java 信息、下载状态和错误文本。文件路径可能间接暴露 Windows 用户名或自定义目录名。用户在 GitHub Issue 中提交日志或截图前，应删除：

- 访问令牌、刷新令牌、授权码或请求头
- Windows 用户名、真实姓名和私人目录名
- 服务器地址、邀请码或不希望公开的实例名称
- 与问题无关的存档、聊天内容或截图

项目维护者只会处理用户主动提交的支持材料。公开 GitHub Issue 默认可被任何人查看。

### 7. 数据保留、删除与备份

- 设置、实例和缓存保留在本机，直到用户通过应用功能或文件系统删除
- 日志可由用户逐个删除，或按设置的保留天数清理
- 未完成下载和缓存可通过存储清理功能或删除对应目录移除
- 卸载应用不一定自动删除 `%APPDATA%\NaCL` 和 `%LOCALAPPDATA%\NaCL`
- 删除实例目录可能同时删除世界存档、截图、资源包和本地配置，操作前应备份重要内容

NaCL 维护者不持有这些本地文件，无法代替用户恢复、导出或远程删除设备上的数据。

### 8. 安全边界

NaCL 0.3.0 是第一阶段公开版本，但仍属于早期软件。当前源码与发布包公开不代表已经完成独立安全审计或代码签名。用户不应把真实令牌手动写入配置文件、启动器日志或 Issue。

如果发现可能泄露账号、令牌或本地文件的安全问题，请避免在公开 Issue 中披露可利用细节；可先创建不包含机密的简短 Issue，请求维护者提供私下联系途径。

### 9. 未成年人

NaCL 不面向特定年龄群收集个人信息，也没有自建注册系统。Microsoft/Xbox/Minecraft 账号的年龄要求、家庭设置和未成年人数据处理由相应服务及其政策管理。监护人应根据所使用账号和在线服务决定是否允许使用。

### 10. 变更与联系

当登录、遥测、更新、崩溃上报、云同步或其他数据处理方式发生变化时，本说明将在相关功能发布前或同时更新。文档顶部的生效日期会随实质变化更新。

隐私问题可以通过本仓库的 [GitHub Issues](https://github.com/Nalongur/NaCL/issues) 提出。请勿在公开 Issue 中提交密码、令牌、授权码或未经脱敏的完整日志。

## English

### 1. Scope and roles

This notice applies to the NaCL desktop application built from this repository. It does not replace the privacy policies of Microsoft, Mojang, Minecraft, the Eclipse Foundation, GitHub, or download-hosting providers.

In the current version:

- the NaCL maintainers receive no in-app registration data because the project operates no account system
- the maintainers do not automatically receive local settings, instances, logs, Java paths, or diagnostics
- when a user requests a third-party service, that provider processes the corresponding network request under its own policy
- when a user submits a GitHub Issue, log, or screenshot, GitHub and the maintainers receive the content the user chooses to publish

### 2. Local data inventory

| Data category | Contents | Purpose | Default location | Retention |
| --- | --- | --- | --- | --- |
| Launcher settings | Theme, default instance, Java selection, memory, and interface preferences | Restore user preferences | `%APPDATA%\NaCL\config` | Until changed or deleted by the user |
| Download settings | Concurrency, retries, timeout, bandwidth limit, and verification settings | Control download behavior | `%APPDATA%\NaCL\config` | Until changed or deleted by the user |
| Storage path settings | User-selected instance installation and shared-cache directories | Store instances and re-downloadable cache in chosen locations | `%APPDATA%\NaCL\config\storage-paths.json` | Until reset or deleted |
| Offline profile | User-entered game username and the stable UUID derived from it | Local offline identity and game launch | `%APPDATA%\NaCL\config\offline-profile.json` | Until changed or deleted by the user |
| Microsoft account profile | Minecraft UUID, player name, and public official skin/cape URLs and states | Display the account and prepare authenticated launch | `%APPDATA%\NaCL\config\microsoft-account.json` | Until sign-out or manual deletion |
| Microsoft refresh token | Long-lived Microsoft refresh token encrypted with Windows current-user DPAPI | Restore sign-in and obtain new short-lived tokens after restart | `%APPDATA%\NaCL\config\microsoft-refresh-token.bin` | Deleted on sign-out; may also be deleted manually |
| Instance metadata | Instance name, Minecraft version, memory, display, and advanced arguments | Manage isolated instances | `%APPDATA%\NaCL\instances\<instance-id>` by default; customizable | Until the instance is deleted |
| Instance game directory | Worlds, screenshots, configuration, and game-generated data | Keep instances isolated | `<instance-id>\game` under the active instance directory | Until deleted or moved by the user |
| Version and asset cache | Version manifests, version JSON, client, libraries, asset indexes, and asset objects | Install and verify vanilla files | `%LOCALAPPDATA%\NaCL\cache` by default; customizable | Until cleaned or deleted by the user |
| Managed Java | Downloaded Eclipse Temurin JREs and runtime information | Provide Java for compatible game versions | `%LOCALAPPDATA%\NaCL\runtimes` | Until cleaned or deleted by the user |
| Temporary downloads | In-progress `.part` files and installation intermediates | Support retries and atomic replacement | `%LOCALAPPDATA%\NaCL\cache\downloads` | Removed on success; failed files may be cleaned by the user |
| Local logs | Launcher, download, installation, game process output, and diagnostic text | Local troubleshooting | `%LOCALAPPDATA%\NaCL\logs` | According to the user-selected retention period or manual deletion |
| System diagnostics | Operating-system type, CPU architecture, physical memory, and directory usage | Java selection, memory recommendations, and local diagnostics | Read at runtime; reports are not uploaded by default | Read only when required by a feature |
| User-selected paths | Java executable and directories explicitly opened or selected by the user | Use a selected runtime or open local storage | Retained when written to the relevant local setting | Until changed or deleted by the user |

NaCL does not intentionally scan documents, browser data, password stores, contacts, or files unrelated to launcher operation. Minecraft worlds and screenshots inside an instance remain local and are not automatically uploaded.

### 3. Network requests and third-party services

Network activity results from user-triggered version refreshes, instance installation, Java downloads, Microsoft sign-in, and game launch. Offline launch does not send the profile to NaCL maintainers, but the Minecraft client may still contact Mojang/Microsoft services or servers selected by the user.

| Service or destination | Trigger | Data sent or exposed | Response |
| --- | --- | --- | --- |
| Mojang version metadata (`piston-meta.mojang.com`) | Refreshing the catalog or reading version metadata | Ordinary HTTPS request information such as IP address, request time, and network client metadata | Version manifest and version JSON |
| Minecraft asset service (`resources.download.minecraft.net`) | Installing or repairing vanilla assets | Ordinary HTTPS request information and the requested hash path | Minecraft asset objects |
| Official file locations referenced by version JSON | Installing the vanilla client and libraries | Ordinary HTTPS request information and requested file paths | Client, libraries, native libraries, and asset indexes |
| Eclipse Adoptium API (`api.adoptium.net`) and download locations | The user requests managed Java | Windows platform, CPU architecture, requested Java major version, and ordinary HTTPS request information | Eclipse Temurin JRE metadata and archives |
| Microsoft, Xbox, and Minecraft identity services | The user initiates sign-in, restores a session, verifies entitlement, or launches with an authenticated account | OAuth requests, short-lived tokens, Xbox/XSTS exchange material, and required account information | Minecraft short-lived token, entitlement, UUID, player name, skin, and cape profile |
| GitHub | The user visits the repository, submits an Issue, or uploads diagnostic material | User-submitted account details, Issue text, attachments, and network metadata recorded by GitHub | Source code, Issues, and support communication |

NaCL does not send Microsoft tokens, offline profiles, or local logs to Eclipse Adoptium, and it does not send Java download information to Mojang. Each provider receives only the requests required for its corresponding operation.

Third parties may process IP addresses, device and browser information, request times, cookies, or account data under their own policies. See:

- [Microsoft Privacy Statement](https://www.microsoft.com/privacy/privacystatement)
- [Eclipse Foundation Website Privacy Policy](https://www.eclipse.org/legal/privacy/)
- [GitHub Privacy Statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement)

### 4. Microsoft sign-in and token design

NaCL's Microsoft sign-in follows these implementation boundaries:

1. Present Microsoft's official authorization page; NaCL will not imitate a password form or ask the user to enter a Microsoft password into the launcher
2. Use an OAuth flow appropriate for a public Windows desktop client, with protections against intercepted or replayed authorization codes
3. Request only the permissions needed for sign-in, Xbox/Minecraft identity exchange, Minecraft ownership checks, profile retrieval, and user-initiated launch
4. Use short-lived access tokens only as needed and keep them out of ordinary settings files and logs
5. Protect refresh tokens with Windows current-user DPAPI and store them separately from ordinary instance configuration; keep Microsoft, Xbox, and Minecraft short-lived access tokens only in process memory
6. Remove locally stored token material when the user signs out; users may also revoke application access through their Microsoft account
7. Never sell tokens, use them for advertising profiles, or provide them to third parties unrelated to authentication or Minecraft services
8. Redact authorization headers, complete tokens, and other login-capable secrets before writing logs or error details

Launching Minecraft requires passing the short-lived Minecraft access token to the local Java process through the arguments defined by official version metadata. While the game runs, sufficiently privileged local process-inspection tools may be able to see that argument. NaCL does not write it to its own logs. The implementation may change with Microsoft or Minecraft API requirements, with this notice updated accordingly.

### 5. Offline profiles

Offline profiles create a display name and stable UUID on the local device for vanilla game launch that does not require official online authentication. Offline profiles:

- do not verify that the user owns Minecraft
- do not bypass services that require Microsoft, Xbox, or Minecraft authentication
- are not automatically uploaded to a maintainer-operated server
- may be incompatible with online-server UUID, skin, permission, or allowlist rules

NaCL passes the offline username, UUID, and empty online-authentication fields to the local Minecraft process. It does not upload the offline profile to a maintainer-operated service. The Minecraft client or a server joined by the user may still process the username and UUID under its own rules.

### 6. Telemetry, logs, and support material

The current version contains no:

- usage analytics or behavior-tracking SDK
- advertising identifier or advertising network
- automatic crash reporting
- background telemetry or remote configuration
- maintainer-operated log upload service

Local logs may contain version numbers, file paths, Java information, download status, and error text. File paths may indirectly reveal a Windows username or custom directory name. Before submitting logs or screenshots in a GitHub Issue, users should remove:

- access tokens, refresh tokens, authorization codes, or authorization headers
- Windows usernames, real names, and private directory names
- server addresses, invitations, or instance names that should remain private
- unrelated worlds, chat content, or screenshots

The maintainers process only support material the user chooses to submit. Public GitHub Issues are visible to anyone by default.

### 7. Retention, deletion, and backup

- settings, instances, and caches remain on the device until removed through application controls or the file system
- logs can be deleted individually or expired according to the configured retention period
- incomplete downloads and caches can be removed with storage-cleanup functions or by deleting their directories
- uninstalling the application may not automatically remove `%APPDATA%\NaCL` and `%LOCALAPPDATA%\NaCL`
- deleting an instance directory may also remove worlds, screenshots, resource packs, and local configuration; important files should be backed up first

The NaCL maintainers do not possess these local files and therefore cannot restore, export, or remotely delete them on the user's behalf.

### 8. Security boundary

NaCL 0.3.0 is the first public Stage 1 release, but it remains early-stage software. Public source and release binaries do not mean that the application has completed an independent security audit or code signing. Users must not manually place real tokens in configuration files, launcher logs, or Issues.

If a security problem could expose an account, token, or local file, avoid posting exploitable details in a public Issue. A short Issue without secrets may be used to request a private contact path.

### 9. Children

NaCL does not operate an account registration system or intentionally collect personal information for a particular age group. Age requirements, family settings, and children's data handling for Microsoft, Xbox, and Minecraft accounts are controlled by those services and their policies. Guardians should decide whether use is appropriate for the account and online services involved.

### 10. Changes and contact

If sign-in, telemetry, updating, crash reporting, cloud synchronization, or other data-handling behavior changes, this notice will be updated before or alongside the relevant release. The effective date at the top will change when the notice changes materially.

Privacy questions may be submitted through this repository's [GitHub Issues](https://github.com/Nalongur/NaCL/issues). Do not include passwords, tokens, authorization codes, or complete unredacted logs in a public Issue.
