# Na Craft Launcher

NaCL 是面向 Windows 的 Minecraft Java Edition 启动器。当前阶段使用
Tauri 2、Vue 3、TypeScript 与 Rust，首版只支持原版游戏实例。

## 源码目录

```text
app/
├─ crates/
│  └─ launcher-core/     # 与界面无关的启动器核心逻辑和数据模型
│     ├─ examples/       # 可独立运行的核心功能示例
│     └─ src/
│        ├─ data.rs      # 用户目录、全局设置与持久化
│        ├─ instance.rs  # 原版实例配置模型
│        ├─ java.rs      # 本机 Java 检测
│        ├─ logs.rs      # 本地日志索引与安全读取
│        └─ system.rs    # 诊断信息与 Windows 目录入口
├─ src/                  # Vue 界面
│  ├─ assets/            # 字体与品牌资源
│  ├─ components/        # 创建、设置和日志侧滑组件
│  └─ styles/            # 主题变量与全局样式
├─ src-tauri/            # Tauri 桌面壳与前后端命令桥接
└─ Cargo.toml            # Rust workspace
```

构建产物统一进入 `app/target/` 和 `app/dist/`，不放入源码模块中。

## Windows 用户数据

需要随用户设置保留、适合备份的数据位于 `%APPDATA%\NaCL`：

```text
NaCL/
├─ config/
│  ├─ settings.json
│  ├─ download-settings.json
│  └─ offline-profile.json
└─ instances/
   └─ <instance-id>/
      ├─ instance.json
      └─ game/
```

可重新下载或自动生成的数据位于 `%LOCALAPPDATA%\NaCL`：

```text
NaCL/
├─ cache/
│  ├─ manifests/
│  ├─ assets/
│  ├─ libraries/
│  └─ downloads/
├─ runtimes/
└─ logs/
```

离线档案只包含用户名和据此生成的稳定 UUID，不包含密码或 Microsoft
令牌。账号令牌不会写入 `settings.json`；实现 Microsoft 登录时将使用 Windows
用户级 DPAPI 加密刷新令牌，短期访问令牌只保留在内存中。
