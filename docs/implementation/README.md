# 实现进度（Implementation Status）

本目录记录**已实现并被测试覆盖**的内容。架构与协议仍以
[调研与架构设计报告](../architecture/RESEARCH_AND_ARCHITECTURE.md) 和
[ADR](../decisions/) 为准；本目录只描述实现事实，不替代设计文档。

相关：[项目简介](../PROJECT.md)（这个项目要做什么）、[品牌化清单](BRANDING.md)（发布前必读）、
[路线图](ROADMAP.md)、[Linux 桌面端排查](LINUX-DEV.md)（窗口空白 / GBM 报错）。

## 流程说明

报告第 34 节要求先评审并冻结 ADR 再进入实现。该前置条件已由
[ADR-0007](../decisions/ADR-0007-readest-as-product-base.md) 取消：架构决策按 Accepted
状态推进，并在需要时以新 ADR 修订，不再阻塞实现。

许可证已确定：ADR-0005 修订版为**全项目 AGPL-3.0**，仓库已包含 `LICENSE`
（与上游 Readest 为同一份 AGPL-3.0 文本），Cargo workspace 亦声明 `license = "AGPL-3.0"`。
本节此前记录为「ADR-0005 未冻结、不声明 license 字段、不添加 LICENSE 文件」，与仓库实际
状态不符，已于 2026-09-16 更正。

**产品底座已改为 Readest 上游 fork**（ADR-0007）。本目录中「自研客户端」相关的进度描述
需按此重新解读：自研 Vite 客户端与 vendor 的 foliate-js 已退役，客户端 UI 以上游实现为准；
[品牌化清单](BRANDING.md) 是发布前的必要工作。

## 仓库现状（2026-09-16，ADR-0007 之后）

| 层 | 位置 | 状态 |
| --- | --- | --- |
| 客户端 UI / 阅读体验 / 六端打包 | `apps/readest-app`（上游实现） | ✅ 可运行，见下方「本地启动」 |
| Rust Core | `crates/`（独立嵌套 workspace） | ✅ 已并入，114 个测试通过、clippy 无警告 |
| 自托管服务端 | `server/` | ❌ 未开始（MVP 3） |
| 品牌化 | 见 [BRANDING.md](BRANDING.md) | ⚠️ P0（上游设施隔离）已完成；P1/P2（签名、生成的原生工程、文案素材）未完成 |

### Rust Core 在仓库中的位置

`crates/` 是一个**独立的嵌套 Cargo workspace**，并通过根 `Cargo.toml` 的 `exclude` 排除在
readest 的 workspace 之外。这样上游的 `Cargo.toml`、`Cargo.lock`、`Cargo.cef.lock` 都保持
不动，`rebase upstream/main` 不会被 Cargo 依赖图冲突波及；同时 Rust Core 可以继续使用
edition 2024 与自己的 lint 集合。

```bash
cd crates
cargo test --offline
cargo clippy --offline --all-targets
```

`reader-protocol` 的契约夹具已随 crate 一起迁移到 `crates/protocol/fixtures/`
（原 `packages/protocol/fixtures/`）；TypeScript 侧的 `packages/protocol` 暂未迁入，
等出现 TS 消费者（例如客户端直连自托管服务端）时再并入。

### 本地启动（2026-09-16 验证）

```bash
git submodule update --init --recursive   # 8 个 submodule，含 readest 的 foliate-js fork
pnpm install --frozen-lockfile
pnpm --filter @readest/readest-app setup-vendors   # simplecc/pdfjs/jieba → public/vendor
pnpm dev-web                                       # http://localhost:3000
```

注意两点，都是缺少后应用无法启动的：

- **submodule 必须先初始化**，`packages/foliate-js`、`packages/tauri`、`packages/simplecc-wasm`
  等既是 pnpm workspace 成员，也通过根 `Cargo.toml` 的 `[patch.crates-io]` 参与 Rust 依赖解析。
- **`setup-vendors` 不是可选项**：缺 `public/vendor/simplecc` 时首页会因
  `@simplecc/simplecc_wasm` 解析失败直接返回 500。

验证结果：`/`、`/library`、`/reader`、`/o` 均返回 200。

## 环境约束

项目所有者已授权安装依赖，因此前端依赖已安装并纳入版本控制（`pnpm-lock.yaml`）：

- 前端：`pnpm install` 可正常联网安装；`pnpm -r run typecheck|test`、`pnpm run build` 均已通过。
- Rust：依赖来自本地 registry 缓存时使用 `--offline`；如果缓存里缺少某个 crate，
  cargo 需要在沙箱外运行才能把依赖解包到 `registry/src`。
- 若再次回到无网环境，`node_modules/` 与 `target/` 均未提交，需要重新安装依赖。

## 已实现

阶段：MVP 1 的 Core 基础（Rust workspace）。骨架见报告第 27 节。

Rust workspace 共 110 个单元测试（另有 1 个被忽略的夹具再生成测试），
`cargo clippy --workspace --all-targets` 无警告；前端另有 41 个测试。

### `crates/reader-model`

| 能力 | 对应文档 | 状态 |
| --- | --- | --- |
| UUIDv7 实体 ID；长度前缀 SHA-256 派生 ID（`spine_item_id`/`text_block_id`） | §19.1 | ✅ 已实现 |
| EPUB CFI 解析/渲染（步骤、断言转义、范围、父引用、字符/时间/空间偏移、side bias） | §10.5 | ✅ 已实现 |
| CFI 规范化与结构等价（`/4,,/20` ≡ `/4/20`） | §20.3 步骤 2 | ✅ 已实现 |
| `spine_index()` 从 CFI 推导阅读顺序 | §19.3、§21.5 | ✅ 已实现 |
| Locator 模型与校验；未知字段透传保留 | §19.3 | ✅ 已实现 |
| 阅读顺序比较（不同 Edition 返回 `Incomparable` 而不是猜测） | §21.5 | ✅ 已实现 |

### `crates/sync`

| 能力 | 对应文档 | 状态 |
| --- | --- | --- |
| HLC 字符串 `<physical>-<counter>-<device>`、单调性、时钟回拨、counter 溢出报错 | §16.3 | ✅ 已实现 |
| 字段信封 `{v,t,s}` 与设备一致性校验 | §16.3 | ✅ 已实现 |
| `mergeReplica`：字段级 LWW、`deletedAt` 取最大、复活令牌、`schemaVersion`/`updatedAt` | §21.4 | ✅ 已实现 |
| 墓碑不可被字段写入复活；相同 HLC 时按值确定性打破平局 | §16.3 | ✅ 已实现 |
| 笔记三方合并（行级）、冲突与超限显式报告 | §21.4、§20.4 | ✅ 已实现 |
| 进度合并：current 用 HLC LWW，furthest 用阅读顺序，附警告 | §21.5 | ✅ 已实现 |
| 章节级进度合并 | §21.5 | ✅ 已实现 |
| `Op`（camelCase 线格式）、payload 哈希、按 `opId` 幂等、cursor 分页 | §16.5、§21.3 | ✅ 已实现 |
| 合并律验证（交换/结合/幂等，200 轮随机副本） | §21.9 | ✅ 已实现 |
| HTTP 传输层（`Transport` trait + `SyncClient` 驱动 cursor 分页、幂等 key、协议级 JSON 形状） | §21.6 | ✅ 已实现 |
| Desktop `ReqwestTransport`（`reqwest::blocking` + `rustls`）+ Tauri commands `sync_push`/`sync_pull` | §21.6 | ✅ 已实现 |
| Web/PWA `FetchSyncTransport`（浏览器原生 `fetch` + `AbortController` 超时） | §21.6 | ✅ 已实现 |
| Transport 自动选择（desktop→Tauri invoke / Web→fetch）+ `pullAll` 分页生成器 | §21.6 | ✅ 已实现 |
| 服务端 Postgres 合并函数、Blob 协议 | §21.6 | ❌ 未实现（MVP 3） |

### `crates/annotation`

| 能力 | 对应文档 | 状态 |
| --- | --- | --- |
| 锚点模型（scheme/state/origin/quote/rects/page/confidence） | §20.2 | ✅ 已实现 |
| 七步重新定位流水线：精确 CFI → 结构 CFI → block+偏移 → 文本引用 → 模糊 → 全书 → orphan | §20.3 | ✅ 已实现 |
| 置信度阈值与状态映射（anchored/needs_review/fuzzy/orphaned） | §20.3、§20.2 | ✅ 已实现 |
| 重复文本用 prefix/suffix 上下文消歧 | §20.3 步骤 4 | ✅ 已实现 |
| 归一化（空白折叠、标点折叠）与偏移映射；超限时拒绝比较 | §20.3 步骤 5 | ✅ 已实现 |
| `mergeAnnotationAnchor`：双 anchored／单 orphan／双 orphan | §21.4 | ✅ 已实现 |
| 机器生成/迁移来源标记 | §20.5 | ✅ 已实现 |
| 从渲染层生成新 CFI | — | ❌ 未实现（属于渲染层，流水线只返回新位置） |

### `crates/document`

| 能力 | 对应文档 | 状态 |
| --- | --- | --- |
| ZIP 中央目录解析；stored/deflate；CRC-32 校验；流式读取 | §10.1 | ✅ 已实现 |
| 安全限额：条目数、单条/总大小、压缩比、加密条目、zip64、路径穿越 | §23.2 | ✅ 已实现 |
| ZIP 写入（stored/deflate）用于测试语料与导出 | §27 `tools/corpus-generator` | ✅ 已实现基础能力 |
| XHTML 文本抽取为块（跳过 head/script/style，合并空白） | §10.2 | ✅ 已实现 |
| container.xml / OPF（metadata、manifest、spine）/ EPUB3 NAV / EPUB2 NCX | §10.1 | ✅ 已实现 |
| EPUB 解析接受 `R: Read + Seek`（`Epub::open_reader`），文件路径便捷入口保留；WASM / 桌面 / 自定义 source 统一入口 | §10.1 | ✅ 已实现 |
| 按需 section 抽取；确定性 block ID 与 section 指纹 | §10.2、§10.3 | ✅ 已实现 |
| TXT 流式扫描、精确字节偏移、BOM/合法性编码探测（GBK 标记为“推测”）、章节识别 | §10.3、§10.4 | ✅ 已实现 |
| TXT 接受内存字节（`TxtDocument::open_bytes`），文件路径便捷入口保留；WASM / 桌面统一入口 | §10.3、§10.4 | ✅ 已实现 |
| 格式识别基于内容而非扩展名 | §10.4 | ✅ 已实现 |
| MOBI/FB2/CBZ/DOCX/Markdown | §10.4 第二阶段 | ❌ 未实现（明确报错） |
| PDF 解析（PDF.js 在渲染层） | §10.4 | ❌ 未实现 |

### `crates/storage`

| 能力 | 对应文档 | 状态 |
| --- | --- | --- |
| §18.1 全部客户端表 + 推荐索引，版本化迁移、事务化、可重复执行 | §18.1 | ✅ 已实现 |
| 检测到更新版本的数据库时拒绝启动（不静默降级） | §17.4 | ✅ 已实现 |
| FTS5 能力探测：不可用时返回 `MissingCapability` 而不是空结果 | §18.4 | ✅ 已实现 |
| publication / annotation+anchor+note_revision / reading_state / setting / sync_outbox / FTS5 仓储 | §18.1 | ✅ 已实现 |
| `sync_outbox` 与 `Op` 打通（入队幂等、失败计数、推送后删除） | §21.4 | ✅ 已实现 |
| edition / book_file / spine_item / nav_node / text_block 仓储 | §18.1 | ✅ 已实现 |
| job / reading_session / stat_daily / glossary_term / ai_run / source / source_rule / plugin 仓储 | §18.1 | ✅ 已实现 |
| 端到端集成测试：构造 fixture EPUB → document.index_all → 写入 edition/spine/text_block → FTS 建索引 → 搜索验证 | §18.4、§10.2 | ✅ 已实现 |
| Postgres 服务端 schema 与合并函数 | §18.2 | ❌ 未实现（`server/`，MVP 3） |

### `crates/protocol` + `packages/protocol`（双端协议契约）

| 能力 | 对应文档 | 状态 |
| --- | --- | --- |
| Locator / Anchor / SectionText / Replica / Op / ProgressState 的规范 JSON 夹具 | §21.3、§22.7 | ✅ 已实现 |
| Rust 侧断言夹具能被真实模型反序列化、且与序列化结果完全一致 | §21.9 | ✅ 已实现 |
| TypeScript 侧解析同一批夹具并做运行时校验，字段错误带精确路径 | §22.7 | ✅ 已实现 |
| `extra` 未知字段在客户端往返中保留（不丢新版字段） | §19.3 | ✅ 已实现 |
| 变更夹具：`cargo test -p reader-protocol -- --ignored regenerate` | — | ✅ 已实现 |

### `packages/reader-web` + `apps/client`

| 能力 | 对应文档 | 状态 |
| --- | --- | --- |
| `ReaderEngine` 适配层接口（引擎可替换，业务不直接依赖 foliate-js） | §9.2、§27 | ✅ 已实现 |
| foliate-js 固定 commit `78914ae` 引入，来源与升级流程记录在 `VENDORED.md` | §9.2、§31 | ✅ 已实现 |
| 引擎返回值到阅读模型的防御性映射（缺失字段返回 null，不猜测位置） | §20.3 | ✅ 已实现 |
| 导入 EPUB、书架、渲染、目录跳转、翻页、进度显示 | §11.5 | ✅ 已实现 |
| 阅读位置持久化（浏览器 localStorage，绑定 `Locator` 协议类型） | §19.3 | ✅ 已实现（浏览器侧） |
| UUIDv7 与本地 edition ID 生成 | §19.1 | ✅ 已实现 |
| Tauri 2 桌面壳骨架（Tauri 配置、capabilities、dialog/fs/opener 原生插件、GTK 环境适配） | §11.4、§27 | ✅ 骨架已实现 |
| PDF 渲染 | §10.4 | ❌ 明确报错（桩文件，见 `VENDORED.md`） |
| Core 的 WASM 绑定与 SQLite WASM/OPFS 存储 | §11.1、§18.5 | ❌ 未实现（当前用 localStorage） |
| 批注创建/迁移到 Core 的 `annotation_anchor` | §20、§32.2 | ❌ 未实现 |
| 排版设置（Readium CSS、字体、主题预设） | §10.6 | ❌ 未实现 |
| Playwright 端到端与黄金测试 | §28.6 | ❌ 未实现 |

### 前端验证方式

```bash
pnpm install
pnpm -r run typecheck
pnpm -r run test
pnpm run build
```

前端共 41 个测试（protocol 11、reader-web 12、client 18）。

## 明确未实现（不得伪实现）

- Tauri 桌面/移动壳、PWA 离线安装；Core 的 WASM 绑定。
- PDF.js / Readium CSS 集成与 PDF 渲染（当前打开 PDF 会明确报错）。
- 书源、AI Gateway、OCR、插件运行时、同步服务端、Job 服务。
- 云端与自托管部分。
- MOBI / FB2 / CBZ / DOCX / Markdown 的解析（调用方会得到明确的 `unsupported`
  错误，不会伪装成功）。前端当前只接受 EPUB（其他扩展名会被拒收并提示）。
- 批注创建、导出、统计、AI/OCR/Wenyi Job。

## 验证方式

```bash
cargo test --offline --workspace
cargo clippy --offline --workspace --all-targets
```

两次运行都需要可写的 `CARGO_HOME`（默认 `~/.cargo`）：cargo 会把尚未解压的
依赖解包到 `registry/src`。若在只读环境里构建，需要把 `CARGO_HOME` 指向可写目录
或使用 `--offline` 之外的缓存副本。

## 下一步（按报告路线图）

1. 冻结 ADR-0001～0006，并确认许可证策略（ADR-0005）。
2. Client：React UI + Tauri 2 壳 + `ReaderEngine` 适配层（foliate-js / Readium CSS /
   PDF.js），需要能安装 npm 依赖的环境。**✅ Tauri 2 壳骨架已搭建，ReaderEngine 适配层已实现**。
3. ~~把 `document` 的派生索引与 `storage` 的 `text_fts` 打通为「导入 → 建索引 → 搜索」链路~~
   **✅ 已打通，见 `reader-storage::tests::integration::import_epub_builds_index_and_searches` 端到端测试**。
4. 渲染层补齐：回写新 CFI、把 `relocate` 结果落到 `annotation_anchor`。
5. MVP 2/3 能力（书源、AI、OCR、Wenyi Job、同步服务端）按报告第 28 节推进。

额外可推进项：
- WASM 绑定 + SQLite WASM/OPFS（让 Core 真正驱动前端）——**Core 已支持内存流，WASM 绑定的前置条件已满足**
- Readium CSS 排版集成（字体、主题预设）——**前端 preset/theme 持久化已实现，对接 readium.ts 需要 WASM 通道**
- 批注创建/迁移到 Core 的 `annotation_anchor`
- Playwright 端到端与黄金测试
- Desktop OpLog 读写 → SyncClient 全链路打通（当前 Transport 层已暴露，OpLog→SyncClient 驱动待接）
- 同步配置/凭据持久化（base_url、device_id、auth token）——需要 storage 层加 settings table
- 同步 UI（settings 面板配置服务器、手动触发 push/pull）
