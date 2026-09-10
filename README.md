# grafana_watch_dog

基于 Tauri 2、React 和 Grafana MCP 的只读 AI 运维日报桌面应用。它把 Prometheus、Loki 和 Grafana Alerting 的信号聚合成健康评分、异常解释、趋势与处置建议。

当前版本是可运行的 MVP 纵向切片：桌面端包含完整产品界面、SQLite 日报持久化、历史回看、官方 Grafana MCP stdio 客户端、MCP/模型设置和离线演示报告。

## 已实现

- 系统健康总览、服务评分、7 天趋势和优先问题队列
- 结构化日报与历史日报详情
- AI 调查交互及 MCP 查询范围说明
- Grafana MCP、AI Provider、定时计划配置界面
- Rust/Tauri 命令层和 SQLite 日报存储
- 官方 `mcp-grafana` 进程托管、MCP initialize 握手、工具发现与工具调用基础能力
- Mantine UI、Tabler Icons、TanStack Query 应用基础设施
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

前后端围绕 `Report` JSON 契约解耦。`src/infrastructure/demo/reportFixtures.ts` 和 Rust 的 `demo_report()` 提供离线数据；接入真实采集器时保持该结构即可，无需改动 UI。

## 官方 Grafana MCP

安装官方服务：

```bash
go install github.com/grafana/mcp-grafana/cmd/mcp-grafana@latest
```

应用默认以如下等效参数启动进程：

```bash
mcp-grafana --transport stdio --disable-write \
  --enabled-tools search,datasource,prometheus,loki,alerting,dashboard
```

Grafana URL 与 Service Account Token 通过子进程环境变量 `GRAFANA_URL` 和 `GRAFANA_SERVICE_ACCOUNT_TOKEN` 注入，不拼接进命令行。点击“测试连接”会实际执行 MCP `initialize` 和 `tools/list`。

## 安全边界

- MVP 只读，不执行重启、修改 Dashboard、SQL 写入等操作。
- MCP 配置必须包含 `--disable-write`。
- 设置文件和 SQLite 都不会持久化 Token/API Key；当前版本关闭应用后需重新输入。生产接入应使用系统 Keychain。
- 推荐 Grafana Service Account 仅授予所需 datasource 的查询权限。

## 项目结构

```text
src/app/                         应用入口、Provider 和顶层组合
src/domain/report/               与框架无关的日报领域模型
src/infrastructure/demo/         可替换的演示数据适配器
src/infrastructure/tauri/        前端到 Tauri 的端口适配器
src-tauri/src/mcp/               MCP 协议、进程与官方服务客户端
src-tauri/src/lib.rs             Tauri 命令、SQLite 和配置组合根
src-tauri/capabilities/          Tauri 最小权限声明
```

## 下一阶段

1. 在现有 MCP `call_tool` 基础上实现 Report Collector，映射 `query_prometheus`、`query_loki_logs` 和 Alerting 工具。
2. 增加 OpenAI-compatible Provider，使用 JSON Schema 校验结构化输出。
3. 把凭据接入 Windows Credential Manager / macOS Keychain / Secret Service。
4. 增加 Tokio 调度器、失败重试、查询审计与报告导出。
