# ADR-0006：Wenyi 集成方式

状态：Proposed（待评审）  
日期：2026-09-13

## 背景

Wenyi 是 MIT 的 Python CLI 翻译流水线，能力成熟；但它不是稳定库 API，iOS 不能运行 Python，长任务也不适合移动端。

## 决策

- Wenyi 只作为独立 Worker/Sidecar/远程容器集成，不内嵌进客户端。
- 固定 Wenyi 版本；通过 CLI、manifest.json、events.jsonl、report.json 等有文档的状态文件集成。
- 生成译文/双语 Edition，不修改原书。
- Job 支持中断/续跑/章节重译；单段重译在 v0.7.0 不承诺。
- 术语表、Review 报告、译文 Edition 作为一等资产。

## 后果

正面：隔离 Python 依赖；移动端可用 Job 控制；原有能力快速接入。

负面：需要 Worker 生命周期管理；上游状态格式变化需要适配；单段重译需上游支持或受控状态操作。

## 合规注意

- BabelDOC bridge 是 AGPL 独立进程。
- MinerU 有附加商业条款与在线服务标注义务。
- PDF 翻译路径必须单独做许可证与隐私审查。
