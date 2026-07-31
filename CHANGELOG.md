# 更新日志 / Changelog

本项目遵循[语义化版本](https://semver.org/lang/zh-CN/)。

This project follows [Semantic Versioning](https://semver.org/).

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
