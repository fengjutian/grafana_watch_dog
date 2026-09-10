# AI Ops Daily

基于 Tauri 2、React 和 Grafana MCP 的只读 AI 运维日报桌面应用。它把 Prometheus、Loki 和 Grafana Alerting 的信号聚合成健康评分、异常解释、趋势与处置建议。

当前版本是可运行的 MVP 纵向切片：桌面端包含完整产品界面、SQLite 日报持久化、历史回看、MCP/模型设置和离线演示报告。真实 Grafana MCP 数据采集与 LLM 调用保留了稳定的数据契约，配置凭据后可继续接入。

## 已实现

- 系统健康总览、服务评分、7 天趋势和优先问题队列
- 结构化日报与历史日报详情
- AI 调查交互及 MCP 查询范围说明
- Grafana MCP、AI Provider、定时计划配置界面
- Rust/Tauri 命令层和 SQLite 日报存储
- `--disable-write` 强制安全检查
- 无 Grafana、无模型凭据时的离线演示模式
- 响应式桌面与窄屏布局

## 本地运行

要求：Node.js 20+、Rust stable，以及 Tauri 2 对应的系统依赖。

```bash
npm install
npm run dev
```

启动桌面应用：

```bash
npm run tauri dev
```

构建前端与桌面安装包：

```bash
npm run build
npm run tauri build
```

## 数据流

```text
Grafana → mcp-grafana (read-only) → Rust collector
        → aggregation/anomaly scoring → structured report JSON
        → SQLite → React UI
```

前后端围绕 `Report` JSON 契约解耦。`src/data.ts` 和 Rust 的 `demo_report()` 提供离线数据；接入真实采集器时保持该结构即可，无需改动 UI。

## 安全边界

- MVP 只读，不执行重启、修改 Dashboard、SQL 写入等操作。
- MCP 配置必须包含 `--disable-write`。
- 设置文件和 SQLite 都不会持久化 Token/API Key；当前版本关闭应用后需重新输入。生产接入应使用系统 Keychain。
- 推荐 Grafana Service Account 仅授予所需 datasource 的查询权限。

## 项目结构

```text
src/                    React UI、类型、演示数据与 Tauri API 适配
src-tauri/src/lib.rs    命令、SQLite、配置与安全校验
src-tauri/capabilities  Tauri 最小权限声明
```

## 下一阶段

1. 实现 MCP stdio 生命周期与 JSON-RPC client，映射 `query_prometheus`、`query_loki_logs` 和 Alerting 工具。
2. 增加 OpenAI-compatible Provider，使用 JSON Schema 校验结构化输出。
3. 把凭据接入 Windows Credential Manager / macOS Keychain / Secret Service。
4. 增加 Tokio 调度器、失败重试、查询审计与报告导出。

