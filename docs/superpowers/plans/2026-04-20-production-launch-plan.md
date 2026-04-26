# PoLE 生产级首发实施计划

> **面向 AI 代理的工作说明：** 必须使用 `superpowers:subagent-driven-development`（推荐）或 `superpowers:executing-plans` 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 将当前仓库从“可编译、可运行的 Rust CLI 原型”推进到“可正式商用上线的跨平台产品”，首发同时覆盖桌面用户与节点运营者，并同时交付安装包、绿色版、桌面 GUI、本地 Web 控制台、服务化、自动更新、签名校验与回滚能力。

**架构：** 保持现有 Rust 协议与节点运行核心不变，新增控制面（本地 HTTP API + GUI）、平台运行层（Windows Service / Linux systemd / 桌面启动器）、更新层（签名 + 清单 + 回滚）、发布层（Windows 安装器 / Linux deb / 绿色版），并用统一目录与配置规范串联。

**技术栈：** Rust（核心、控制面、更新器、服务管理）、桌面壳（Tauri 优先）、本地 Web GUI（浏览器技术栈）、Windows MSI、Linux deb、systemd、Windows Service、代码签名/包签名、CI/CD 发布流水线。

---

## 文件结构

**现有核心文件：**
- 修改：`src/bin/pole-client.rs` - 保留用户入口 CLI，但逐步收缩为控制入口与诊断入口
- 修改：`src/bin/pole-node.rs` - 保留节点服务 CLI，但逐步收缩为服务/守护进程入口
- 修改：`src/lib.rs` - 暴露新增公共模块
- 修改：`src/node_config.rs` - 统一安装目录、日志目录、数据目录、更新目录配置
- 修改：`src/node_daemon.rs` - 服务模式与后台运行统一状态管理
- 修改：`src/node_settlement.rs` - 保持治理/状态导出稳定，避免 GUI 直接读文件格式

**建议新增文件/目录：**
- 创建：`src/app_paths.rs` - 统一平台目录解析（config/data/logs/cache/update）
- 创建：`src/service_runtime.rs` - 后台服务生命周期与平台无关状态机
- 创建：`src/service_windows.rs` - Windows Service 安装/启动/停止/卸载
- 创建：`src/service_systemd.rs` - systemd unit 生成、安装、启停与状态检测
- 创建：`src/control_api.rs` - 本地控制面 HTTP API
- 创建：`src/control_api_types.rs` - GUI / API 共享请求响应模型
- 创建：`src/update_manifest.rs` - 版本清单、渠道、签名、回滚元数据
- 创建：`src/updater.rs` - 更新流程协调器
- 创建：`src/signing.rs` - 更新包与发布清单签名校验
- 创建：`src/install_layout.rs` - 安装布局、绿色版布局、迁移策略
- 创建：`desktop/` - 桌面 GUI 壳与前端工程
- 创建：`packaging/windows/` - MSI、快捷方式、服务注册脚本/模板
- 创建：`packaging/linux/deb/` - deb 控制文件、postinst/prerm、systemd 模板
- 创建：`dist/release-manifests/` - 构建产物清单输出
- 创建：`tests/production_paths.rs` - 平台目录与安装布局测试
- 创建：`tests/service_runtime.rs` - 服务生命周期与恢复测试
- 创建：`tests/update_flow.rs` - 更新/签名/回滚测试
- 创建：`tests/control_api.rs` - 控制面 API 合约测试
- 创建：`docs/operations/` - 运维与发布文档

---

## 任务 1：统一平台目录与安装布局

**文件：**
- 创建：`src/app_paths.rs`
- 创建：`src/install_layout.rs`
- 修改：`src/node_config.rs`
- 测试：`tests/production_paths.rs`

- [ ] **步骤 1：编写失败的目录解析测试**

```rust
#[test]
fn resolves_windows_install_layout() {
    let layout = resolve_install_layout(
        Platform::Windows,
        InstallMode::Installed,
        "C:\\Program Files\\PoLE",
    );
    assert!(layout.config_dir.ends_with("PoLE\\config"));
    assert!(layout.log_dir.ends_with("PoLE\\logs"));
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cargo test --quiet production_paths --test production_paths`
预期：FAIL，缺少目录解析模块或断言不满足

- [ ] **步骤 3：实现平台目录与安装布局**

```rust
pub struct InstallLayout {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    pub update_dir: PathBuf,
}

pub fn resolve_install_layout(
    platform: Platform,
    mode: InstallMode,
    root: impl AsRef<Path>,
) -> InstallLayout {
    // 按平台和分发模式统一推导目录
}
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cargo test --quiet production_paths --test production_paths`
预期：PASS

- [ ] **步骤 5：Commit**

```bash
git add src/app_paths.rs src/install_layout.rs src/node_config.rs tests/production_paths.rs
git commit -m "Unify platform directory resolution for production packaging"
```

## 任务 2：统一服务生命周期状态机

**文件：**
- 创建：`src/service_runtime.rs`
- 修改：`src/node_daemon.rs`
- 修改：`src/bin/pole-client.rs`
- 修改：`src/bin/pole-node.rs`
- 测试：`tests/service_runtime.rs`

- [ ] **步骤 1：编写服务生命周期失败测试**

```rust
#[test]
fn restarts_stale_service_state_cleanly() {
    let mut runtime = ServiceRuntime::default();
    runtime.mark_starting(1001);
    runtime.mark_stale();
    assert!(runtime.can_recover_without_manual_cleanup());
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cargo test --quiet service_runtime --test service_runtime`
预期：FAIL

- [ ] **步骤 3：实现服务无关状态机**

```rust
pub enum ServiceState {
    Stopped,
    Starting { pid: u32 },
    Running { pid: u32 },
    Failed { last_error: String },
}
```

- [ ] **步骤 4：接入现有后台运行元数据**

```rust
pub fn sync_with_process_table(&mut self, is_running: bool) {
    // 将 pid 文件、daemon metadata、平台服务状态统一映射
}
```

- [ ] **步骤 5：运行测试验证通过**

运行：`cargo test --quiet service_runtime --test service_runtime`
预期：PASS

## 任务 3：Windows Service 与 Linux systemd 正式接入

**文件：**
- 创建：`src/service_windows.rs`
- 创建：`src/service_systemd.rs`
- 修改：`src/bin/pole-node.rs`
- 修改：`src/bin/pole-client.rs`
- 创建：`packaging/linux/deb/pole-node.service`
- 测试：`tests/service_runtime.rs`

- [ ] **步骤 1：添加服务命令入口测试**

```rust
#[test]
fn service_install_command_is_exposed() {
    let output = Command::new(binary)
        .arg("service-install")
        .arg(config_path)
        .output()
        .unwrap();
    assert!(output.status.success());
}
```

- [ ] **步骤 2：实现平台服务适配器**

```rust
pub trait ServiceManager {
    fn install(&self, config: &NodeConfig) -> Result<()>;
    fn start(&self) -> Result<()>;
    fn stop(&self) -> Result<()>;
    fn status(&self) -> Result<ServiceStatus>;
}
```

- [ ] **步骤 3：为 Linux 生成 systemd unit**

```ini
[Service]
ExecStart=/opt/pole/pole-node service-run /etc/pole/node.json
Restart=on-failure
```

- [ ] **步骤 4：运行平台无关测试与 CLI 暴露测试**

运行：`cargo test --quiet service_runtime --test service_runtime`
预期：PASS

## 任务 4：本地控制面 API

**文件：**
- 创建：`src/control_api.rs`
- 创建：`src/control_api_types.rs`
- 修改：`src/bin/pole-client.rs`
- 修改：`src/bin/pole-node.rs`
- 测试：`tests/control_api.rs`

- [ ] **步骤 1：定义 API 合约测试**

```rust
#[test]
fn status_endpoint_returns_service_and_node_health() {
    let response = api_client.get("/api/status").send().unwrap();
    assert_eq!(response.status(), 200);
    assert!(response.text().unwrap().contains("service_state"));
}
```

- [ ] **步骤 2：实现本地控制面**

```rust
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/status", get(get_status))
        .route("/api/config", get(get_config).post(update_config))
        .route("/api/logs", get(get_logs))
}
```

- [ ] **步骤 3：限制默认监听到本机**

```rust
let bind_addr = if remote_enabled {
    "0.0.0.0:..."
} else {
    "127.0.0.1:..."
};
```

- [ ] **步骤 4：运行 API 测试**

运行：`cargo test --quiet control_api --test control_api`
预期：PASS

## 任务 5：GUI 双入口（桌面壳 + 浏览器）

**文件：**
- 创建：`desktop/`
- 创建：`desktop/src-tauri/`
- 创建：`desktop/web/`
- 修改：`packaging/windows/`
- 测试：前端/集成测试脚本

- [ ] **步骤 1：创建 GUI 外壳与本地 Web 共享路由**
- [ ] **步骤 2：实现首页、状态页、配置页、日志页、更新页**
- [ ] **步骤 3：桌面壳默认打开本地控制台**
- [ ] **步骤 4：浏览器直接访问同一控制面**
- [ ] **步骤 5：加入访问开关与远程访问配置 UI**

运行：`cargo test --quiet && [frontend test command]`
预期：PASS

## 任务 6：更新、签名与回滚

**文件：**
- 创建：`src/update_manifest.rs`
- 创建：`src/updater.rs`
- 创建：`src/signing.rs`
- 创建：`tests/update_flow.rs`
- 创建：`dist/release-manifests/`

- [ ] **步骤 1：编写更新清单与签名失败测试**
- [ ] **步骤 2：定义统一清单格式**

```json
{
  "channel": "stable",
  "version": "1.0.0",
  "artifacts": [...],
  "signature": "..."
}
```

- [ ] **步骤 3：实现下载、校验、切换与回滚**
- [ ] **步骤 4：服务模式下安全更新**
- [ ] **步骤 5：运行更新测试**

运行：`cargo test --quiet update_flow --test update_flow`
预期：PASS

## 任务 7：Windows MSI 与 Linux deb 打包

**文件：**
- 创建：`packaging/windows/`
- 创建：`packaging/linux/deb/`
- 创建：`scripts/release/`
- 修改：CI 工作流

- [ ] **步骤 1：定义安装产物目录布局**
- [ ] **步骤 2：Windows MSI 打包脚本**
- [ ] **步骤 3：Linux deb 控制文件与 postinst/prerm**
- [ ] **步骤 4：绿色版压缩包生成**
- [ ] **步骤 5：签名接入**

运行：`[packaging build commands]`
预期：生成 MSI、deb、zip/tar.gz

## 任务 8：桌面快捷方式、首次启动与诊断体验

**文件：**
- 修改：`dist/click-to-run/`
- 修改：`scripts/`
- 修改：`src/bin/pole-client.rs`
- 测试：现有 CLI 集成测试 + 新增启动器测试

- [ ] **步骤 1：首次启动自动初始化**
- [ ] **步骤 2：安装版创建快捷方式**
- [ ] **步骤 3：绿色版生成启动器**
- [ ] **步骤 4：错误时展示明确诊断与日志位置**

运行：`cargo test --quiet`
预期：PASS

## 任务 9：发布流水线与文档

**文件：**
- 创建：`.github/workflows/release.yml`
- 创建：`docs/operations/install.md`
- 创建：`docs/operations/update.md`
- 创建：`docs/operations/service-management.md`
- 创建：`docs/operations/troubleshooting.md`

- [ ] **步骤 1：定义发布流水线**
- [ ] **步骤 2：生成版本清单与签名**
- [ ] **步骤 3：上传安装包与绿色版**
- [ ] **步骤 4：补齐用户/运维文档**

运行：`[CI dry-run or local workflow validation]`
预期：PASS

---

## 收尾验收清单

- [ ] Windows 安装包可安装、可卸载、可升级
- [ ] Linux deb 可安装、可升级、可卸载
- [ ] 绿色版可直接解压运行
- [ ] Windows Service 可管理
- [ ] Linux systemd 可管理
- [ ] 桌面 GUI 与浏览器 GUI 共用控制面
- [ ] 默认仅本机访问，远程访问需显式开启
- [ ] 自动更新、签名校验、回滚完整闭环
- [ ] 全量测试、lint、typecheck、打包检查通过
- [ ] 发布文档完整

## 建议执行方式

首选：`superpowers:subagent-driven-development`

原因：
- 本计划天然可拆为平台层、控制面、更新层、打包层 4 条相对独立的并行线
- 每条线都能以 disjoint write scope 执行，降低冲突
- 阶段中间保留 review/checkpoint，更适合正式商用上线节奏
