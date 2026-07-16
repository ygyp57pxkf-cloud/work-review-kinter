# Work Journal 打包与发布运行手册

## 发布范围

GitHub Actions 的 `.github/workflows/release.yml` 支持两种入口：手动 `workflow_dispatch` 用于内部无 updater 签名构建；推送 `v*` 标签用于正式发布候选。当前矩阵包含：

| 平台 | Rust target | 交付物 |
| --- | --- | --- |
| macOS Apple Silicon | `aarch64-apple-darwin` | DMG、带 `aarch64` 后缀的 `.app.tar.gz` 自动更新包及 `.sig`。 |
| macOS Intel | `x86_64-apple-darwin` | DMG、带 `x64` 后缀的 `.app.tar.gz` 自动更新包及 `.sig`。 |
| Windows x64 | `x86_64-pc-windows-msvc` | NSIS 安装程序 `.exe` 及 `.sig`，另附 portable ZIP。 |
| Linux x64 | `x86_64-unknown-linux-gnu` | AppImage、DEB、RPM。 |
| Linux ARM64 | `aarch64-unknown-linux-gnu` | DEB。 |

Windows portable ZIP 名为 `Work_Journal_portable_x64.zip`，包含 `Work_Journal.exe` 与 `PORTABLE_README.txt`，采用 PowerShell `Compress-Archive` 生成。它依赖 WebView2；Windows 10/11 通常自带，旧系统需要自行安装。ZIP 是免安装分发物，不是 updater 的签名安装包。

## 本机 macOS 构建

在仓库根目录执行：

```bash
npm ci
npm run tauri:build:local-mac
```

该脚本构建 Apple Silicon (`aarch64-apple-darwin`) 的 `.app`，并使用 `src-tauri/tauri.local.conf.json` 关闭 updater 产物和 macOS 签名身份要求，适用于本机验收。它不替代 Intel macOS 构建，也不产生可发布的自动更新签名包。Intel 产物由发布工作流的对应矩阵任务构建。

## Secrets 与密钥纪律

工作流只从 GitHub Actions Secrets 读取下列名称，仓库、文档、日志、Release note 和产物说明中均不得写入其值：

- `MACOS_CODESIGN_P12`：可选的 Base64 macOS P12 证书。
- `MACOS_CODESIGN_PASSWORD`：上述 P12 的密码。
- `TAURI_SIGNING_PRIVATE_KEY`：Tauri updater 签名私钥。
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`：Tauri updater 签名私钥密码。
- `GITHUB_TOKEN`：创建或更新 GitHub Release 时使用。

若 macOS P12 未设置、为空或导入失败，工作流会给出 warning 并继续；构建步骤目前对 macOS 统一设定 `APPLE_SIGNING_IDENTITY=-`，即 ad-hoc 身份。因此“证书已配置”不等于已验证为正式 Developer ID 签名。Windows 的 `.sig` 是 Tauri 自动更新签名，不应表述为已完成 Windows Authenticode 代码签名，除非另有真实验收证据。

## GitHub Actions 验证与发布检查

每个矩阵任务按以下顺序执行并失败即停止：

1. `npm ci`、`node --test`。
2. `npm run build`，为 Rust 测试生成前端产物。
3. `cargo test --manifest-path src-tauri/Cargo.toml`。
4. `npm run tauri build -- <matrix args>`；手动内部构建会额外使用 `src-tauri/tauri.local.conf.json` 关闭 updater 产物签名。
5. 验证所需产物：内部构建要求两种 macOS 架构的 DMG、Windows NSIS `.exe`、portable ZIP 和对应 Linux 包；tag 构建还要求 macOS/Windows updater 包及 `.sig`。
6. 上传构建产物；只有 `v*` tag push 才运行发布 job。正式发布 job 下载全部产物，要求 macOS ARM、macOS Intel、Windows x64 三种 updater 资产及签名齐全，生成三个 `updater*.json` 后创建或更新 GitHub Release。

内部构建可从 Actions 页面运行 `Release` workflow，或在已认证的 GitHub CLI 中执行：

```bash
gh workflow run release.yml --ref feature/integrated-work-journal
```

手动运行不会创建 GitHub Release，也不应被解释为正式签名发布。

对发布候选，先确认 tag 对应的 Actions run 全绿、各矩阵资产存在、`updater.json` 的三个平台条目齐全，再下载对应平台产物进行人工安装或启动验收。

## 真实启动状态

**待确认：真实 Windows x64 环境的安装包与 portable ZIP 启动验收。** 内部工作流会构建并检查 Windows NSIS 产物和 portable ZIP；正式 tag 路径才额外要求 updater `.sig`。这些都是编译和产物结构检查，不是实际启动测试。完成真实 Windows 验收后，应记录系统版本、WebView2 状态、安装包/便携版、首次启动结果和验证日期；在此之前不得把 Windows 启动写为已验证。
