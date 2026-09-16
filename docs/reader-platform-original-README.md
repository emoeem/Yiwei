# Reader Platform（工作名）

跨平台、Local-first、插件化的现代电子书/网络小说阅读平台。

当前阶段：**MVP 1 — Core 基础实现**。Phase 0 的调研报告与 ADR 已完成，项目所有者已要求
进入实现，因此 Rust Core workspace 已开始落地。ADR-0001～ADR-0006 仍为 `Proposed`，
正式冻结（评审通过）仍是未完成的动作项，详见 [实现进度](docs/implementation/README.md)。

## 文档

- [跨平台现代阅读器技术调研与架构设计报告](docs/architecture/RESEARCH_AND_ARCHITECTURE.md)
- [调研证据与来源清单](docs/research/EVIDENCE.md)
- [架构决策记录（ADR）](docs/decisions/)
- [实现进度与验证方式](docs/implementation/README.md)

## 代码结构

```text
crates/               # Rust Core（客户端与服务端共享）
├── reader-model/    # ID、EPUB CFI、Locator、阅读顺序
├── sync/            # HLC、字段级 LWW、合并规则
├── annotation/      # 锚点与重新定位
├── document/        # ZIP/EPUB 解析、派生索引、TXT 分块
├── storage/         # SQLite schema 与仓储
└── protocol/        # 与 TypeScript 共享的协议夹具与契约测试

packages/             # 前端共享库
├── protocol/        # 协议类型与运行时校验（夹具由 Rust 生成）
└── reader-web/      # ReaderEngine 适配层 + 固定 commit 的 foliate-js

apps/
└── client/          # React + Vite 阅读器（Web 构建，Tauri 壳复用同一产物）
```

前端的构建与测试（需要网络安装依赖）：

```bash
pnpm install
pnpm -r run typecheck && pnpm -r run test && pnpm run build
```

当前前端已实现：导入 EPUB → 书架 → 用 foliate-js 渲染 → 目录跳转 / 翻页 →
记录阅读位置（浏览器 localStorage，Core SQLite 的 WASM 绑定尚未接入）。

当前 Core 已实现：确定性 ID、EPUB CFI、Locator 与阅读顺序；HLC、字段级 LWW、
墓碑/复活、笔记三方合并与进度合并；锚点重新定位与锚点合并；ZIP/EPUB/XHTML/TXT
解析与派生索引；SQLite schema 迁移与仓储（含 FTS5 能力探测）。
详见 [实现进度](docs/implementation/README.md)。当前共 148 个测试（Rust 107 + 前端 41）。

Core 的构建与测试：

```bash
cargo test --workspace
```

## 项目原则

1. 本地数据库是 Source of Truth，云服务只做同步、备份和中继。
2. 原始书籍文件不可变；内部模型是可重建索引，不是唯一真相。
3. AI、OCR、书源、同步、渲染引擎都通过接口接入，不写死在核心。
4. 插件默认无系统权限；权限在宿主边界强制执行，而不是靠“信任插件”。
5. 不以牺牲长期可维护性换取 MVP 速度。
6. 不存在伪实现：未完成的能力必须明确返回“不支持”，不能伪造成功。

名称 `Reader Platform` 只是工作名，不参与技术设计。候选名见报告第 1 节，正式使用前必须做商标检索。
