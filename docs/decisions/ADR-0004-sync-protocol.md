# ADR-0004：Sync Protocol

状态：Proposed（待评审）  
日期：2026-09-13

## 背景

Local-first 多设备同步需要处理离线、时钟不同步、并发编辑与标注位置漂移；简单 LWW 会丢数据。

## 决策

- 本地数据库是 Source of Truth；服务器只做同步/备份/中继。
- 使用 HLC + 字段级 LWW + 墓碑 + remove-wins。
- 标注使用实体字段合并 + 锚点重定位 + orphan 队列，不静默删除。
- 笔记正文保留 revision，并发编辑使用三方合并，失败创建冲突副本。
- 进度分 current LWW 与 furthest 取更远者。
- 书籍文件内容寻址，默认不同步，可选上传；支持 E2EE envelope。
- 客户端与服务端共享 merge 语义，并有 contract tests。

## 考虑的替代方案

- 纯 Last Write Wins：实现简单，但会丢批注和笔记。
- 纯 CRDT 文本：对标注对象与阅读进度不合适，复杂度高。
- 事件溯源全量重放：审计强但存储与恢复成本高。

## 后果

正面：离线可用、冲突可解释、标注可恢复、协议可演进。

负面：实现复杂度高；需要 HLC 属性测试、服务端 merge 函数与冲突 UI。
