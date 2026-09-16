<div align="center">
  <img src="apps/readest-app/public/icon.png" alt="一苇 · Yiwei" width="128" />
  <h1>一苇 · Yiwei</h1>
  <p><strong>以阅读器为核心、Local-first、跨平台、插件化的现代阅读平台</strong></p>
  <p><em>A local-first, plugin-based, cross-platform reading platform.</em></p>
</div>

> 本仓库是 [Readest](https://github.com/readest/readest) 上游的 fork，正在改造成自有产品
> **一苇 / Yiwei**：应用标识 `com.yiwei.reader`、深链 `yiwei://`、二进制名 `yiwei`。
> 技术目录名暂时沿用上游，品牌化进度见[品牌化清单](docs/implementation/BRANDING.md)。

## 这是什么

一句话：**本地电子书与网络小说都能读，支持书源、AI、OCR、翻译、批注，以及官方云 / 自托管同步。**

它不是 EPUB 阅读器的换皮，也不是某个现成客户端的跨平台移植。真正的资产是三样：

1. **阅读体验** —— 排版、字体、翻页、批注、跨设备阅读连续性。
2. **开放协议** —— Document / Annotation / Sync / Plugin / Provider。
3. **生态** —— 书源、AI Provider、OCR Provider、词典、TTS、同步后端、插件。

定位、目标用户、非目标与设计取舍的完整说明见 **[docs/PROJECT.md](docs/PROJECT.md)**。

## 现在的状态

| 阶段 | 内容 | 状态 |
| --- | --- | --- |
| B0 | 上游设施隔离（云 / 遥测 / 更新源 / OAuth）、应用身份与图标、Rust Core 并入 `crates/` | 进行中 |
| B1 | 文档格式与解析能力：PDF 进入自有文档模型、Markdown / HTML / TXT 的语义与索引 | 下一步 |
| B2 | 书源引擎、权限化插件宿主、翻译 Job、自托管同步服务端 | 未开始 |

**尚未发布**：没有官方下载渠道、没有官方云服务，更新源与云配置目前指向保留域名（`*.invalid`）。

## 架构

共享 Rust Core + 同一套 Web Reader UI + Tauri 2 原生壳 + 独立 Web/PWA 壳：

- **渲染层**：EPUB / FB2 / MOBI / CBZ 用 [foliate-js](https://github.com/readest/foliate-js)，
  PDF 用 [PDF.js](https://github.com/mozilla/pdf.js)，排版基线用
  [Readium CSS](https://github.com/readium/readium-css)。
- **Rust Core**（`crates/`）：解析索引、数据库、标注锚点、同步合并、插件宿主，以及 AI / OCR / 书源的安全边界；
  Web 端以 WASM 形式复用。
- **原生壳**：Tauri 2，覆盖 Windows / macOS / Linux / Android / iOS；Web 端构建静态 PWA，复用同一套 UI。
- **服务端**：Rust + Axum + PostgreSQL + S3 兼容对象存储（未开始），官方云与自托管共用一套 Sync Protocol。

目录结构与设计原则见 [docs/PROJECT.md](docs/PROJECT.md)。

## 文档

| 想知道什么 | 看哪里 |
| --- | --- |
| 这个项目要做什么 | [docs/PROJECT.md](docs/PROJECT.md) |
| 为什么这么设计（完整调研与论证） | [docs/architecture/RESEARCH_AND_ARCHITECTURE.md](docs/architecture/RESEARCH_AND_ARCHITECTURE.md) |
| 已拍板的架构决策 | [docs/decisions/](docs/decisions/) |
| 实现进度与验证方式 | [docs/implementation/README.md](docs/implementation/README.md) |
| 当前阶段要做什么 | [docs/implementation/ROADMAP.md](docs/implementation/ROADMAP.md) |
| 发布前的品牌化与去上游化清单 | [docs/implementation/BRANDING.md](docs/implementation/BRANDING.md) |
| Linux 桌面端窗口空白 / GBM 报错 | [docs/implementation/LINUX-DEV.md](docs/implementation/LINUX-DEV.md) |

## 本地开发

```bash
git submodule update --init --recursive
pnpm install --frozen-lockfile
pnpm --filter @readest/readest-app setup-vendors   # simplecc / pdfjs / jieba → public/vendor
pnpm dev-web                                       # http://localhost:3000
pnpm tauri dev                                     # 桌面端（Linux 上走 CEF 运行时）
```

Rust Core：

```bash
cd crates && cargo test --offline
cd crates && cargo clippy --offline --all-targets
```

## 与上游的关系

- 客户端实现以上游 [Readest](https://github.com/readest/readest) 为底座（见
  [ADR-0007](docs/decisions/ADR-0007-readest-as-product-base.md)），并持续 rebase 上游改动。
- 上游 README 原文存档在 [docs/upstream-readme.md](docs/upstream-readme.md)，其中的下载渠道、赞助与
  官方云服务都属于上游项目，与本项目无关。
- 许可证：**AGPL-3.0**（见 [LICENSE](LICENSE)），与上游一致。
