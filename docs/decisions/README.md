# 架构决策记录（ADR）

所有 ADR 初始状态为 `Proposed`。只有评审通过后才能改为 `Accepted`，实现必须遵循 `Accepted` 的 ADR。

| ADR | 主题 | 状态 |
| --- | --- | --- |
| [ADR-0001](ADR-0001-platform-architecture.md) | 跨平台架构：Rust Core + React UI + Tauri 2 + Web/PWA | Proposed |
| [ADR-0002](ADR-0002-document-model-and-rendering.md) | Document Model 与渲染：原文件不变 + 派生索引 | Proposed |
| [ADR-0003](ADR-0003-plugin-runtime-and-platform-policy.md) | 插件运行时与平台策略 | Proposed |
| [ADR-0004](ADR-0004-sync-protocol.md) | Sync Protocol：HLC + 字段级 LWW + 墓碑 | Proposed |
| [ADR-0005](ADR-0005-license-strategy.md) | 许可证策略：Apache-2.0 客户端 + AGPL-3.0 服务端 | Proposed |
| [ADR-0006](ADR-0006-wenyi-integration.md) | Wenyi 集成：独立 Worker，不内嵌 | Proposed |

## 变更流程

1. 新 ADR 使用 `Proposed` 状态提交。
2. 评审记录写入 ADR 的“决策/后果”。
3. 通过后改为 `Accepted`；被替代的 ADR 改为 `Superseded by ADR-XXXX`。
4. 实现必须引用相关 ADR；发现冲突时先更新 ADR，不直接改代码。
