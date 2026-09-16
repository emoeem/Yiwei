# 路线图

原则：先让基础可用、可验证，再动结构。当前阶段（B0）只做身份、图标与工程收尾，
不引入新的解析能力；能力扩展排在 B1。

## B0 — 基础收尾（进行中）

- [x] Readest 上游作为产品底座（ADR-0007）
- [x] Rust Core 并入 `crates/`（嵌套 workspace，114 测试通过）
- [x] 上游设施隔离（云 / 遥测 / 更新源 / OAuth）
- [x] 应用身份：`com.yiwei.reader`、`Yiwei`、深链 `yiwei://`、数据目录 `Yiwei`
- [x] 原生工程：Android applicationId 与 Kotlin 包路径、Xcode 工程与 target 改名
- [x] 图标与启动图（源图在 `data/icons/yiwei/`）
- [ ] 桌面端可运行验证：`pnpm dev-web` 已通过；`pnpm tauri dev` 需 Rust 1.95+ 与系统 WebKit 依赖，尚未跑
- [ ] 在 macOS 上验证 Xcode 工程（`xcodebuild -list`，必要时 `xcodegen generate`）
- [ ] 剩余文案与素材（见 [BRANDING.md](BRANDING.md) 的 P2）

## B1 — 文档格式与解析能力

**先纠正一个前提**：EPUB / MOBI / AZW3 / FB2 / CBZ / TXT / **PDF** / **Markdown** 在上游已经支持，
`SUPPORTED_BOOK_EXTS` 就是这份清单；HTML 也已有路径（经「发送到书架」的转换管线 → Readability → 生成 EPUB）。
所以 B1 不是「从零添加 PDF/MD 解析」，而是补齐我们自己的那一层：

| 方向 | 现状 | 要做的事 |
| --- | --- | --- |
| PDF | 上游用 PDF.js 渲染（`public/vendor/pdfjs`） | 让 PDF 进入我们自己的文档模型：分页/页码定位、文字层抽取、用于批注的锚点（当前锚点是 CFI，PDF 需要页+矩形方案） |
| Markdown | 上游可导入并转换 | 决定语义：是按纯文本书渲染，还是解析标题层级生成目录与章节索引；后者要进 `crates/document` |
| HTML | 仅经转换管线间接支持 | 支持直接打开 `.html`/`.htm`：安全清洗（已有 `utils/sanitize`）、单文件与资源目录两种形态、相对资源解析 |
| TXT | 上游与 `crates/document::txt` 都有 | 合并实现，10,000 章量级的分块与索引走 Rust 侧，前端只取窗口 |
| 其它（DOCX/MOBI 深入解析） | 上游可用但精度有限 | 按需排期，不做承诺 |

判断标准：B1 的每一项都必须能回答「这项工作属于客户端渲染，还是属于可重建的派生索引」——
前者交给上游的渲染层，后者才进 Rust Core（对应 ADR-0007 的职责划分）。

## B2 — 差异化能力

按「上游没有、且与现有数据层耦合最小」排序：

1. **书源引擎**（Legado 规则 L0/L1、受限运行时）。上游 `services/novel` 只是「一个 URL → Readability → EPUB」，
   不是规则引擎；这是第一个 Rust 集成点，也最容易验证 Rust → TS 两条链路（Tauri command + WASM）。
2. **权限化插件宿主**（ADR-0003）：上游 `services/plugins` 是内置插件机制，不是权限边界与沙箱。
3. **Wenyi 翻译 Job**（ADR-0006）与 OCR/漫画翻译：Job 状态机 + Sidecar，不做伪实现。
4. **自托管同步服务端**（ADR-0004 + `server/`）：复用 `crates/{sync,storage,protocol}`，客户端继续用上游的
   Turso/TS 实现，服务端只提供协议端点。

## 待定事项

- 商标与域名检索（`一苇 / Yiwei`，CNIPA 第 9/41/42 类）
- 是否重命名 Rust crate（`Readest` → 自有名）与包 scope `@readest/*`
- 是否保留 `src-tauri/icons/android/**`（上游遗留、当前未被构建引用）
