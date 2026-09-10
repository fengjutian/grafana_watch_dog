# grafana_watch_dog

基于 Tauri 2、React 和 Grafana MCP 的只读 AI 运维日报桌面应用。它把 Prometheus、Loki 和 Grafana Alerting 的信号聚合成健康评分、异常解释、趋势与处置建议。

当前版本是可运行的 MVP 纵向切片：桌面端包含产品界面、SQLite 持久化、官方 Grafana MCP stdio 客户端、MCP/模型设置和定时指标告警。项目不提供占位运行数据；所有展示结果必须来自真实采集和持久化记录。

## 已实现

- 系统健康总览、服务评分、7 天趋势和优先问题队列
- 结构化日报与历史日报详情
- Grafana MCP、AI Provider、定时计划配置界面
- 基于真实告警事件的 OpenAI-compatible AI 异常分析
- 告警历史页面、实时事件更新与触发/恢复记录
- MCP 指数退避重试和配置、握手、工具发现、Grafana 鉴权分阶段诊断
- Rust/Tauri 命令层和 SQLite 日报存储
- 官方 `mcp-grafana` 进程托管、MCP initialize 握手、工具发现与工具调用基础能力
- MCP 页面一键安装：优先复用已有程序，其次使用官方推荐的 `uvx`，最后通过 Go 安装到应用私有工具目录
- Mantine UI、Tabler Icons、TanStack Query 应用基础设施
- `--disable-write` 强制安全检查
- 无数据时的明确空状态和真实错误提示
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

前后端围绕 `Report` JSON 契约解耦。SQLite 中没有真实日报时，界面展示空状态。当前日报采集器尚未接入，调用生成命令会返回明确错误，不会写入占位记录。

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

也可以直接点击 MCP 页面中的“安装 mcp-grafana”。应用不会申请管理员权限：如果系统有 `uvx`，会使用官方推荐的零配置方式准备服务；如果系统有 Go，则把官方二进制安装到应用数据目录并自动回填命令。安装完成后，有完整 Grafana 凭据时会自动连接，否则提示补充 Token。

## 安全边界

- MVP 只读，不执行重启、修改 Dashboard、SQL 写入等操作。
- MCP 配置必须包含 `--disable-write`。
- Token 和 API Key 仅保存到操作系统 Keychain，不写入设置文件或 SQLite。
- 推荐 Grafana Service Account 仅授予所需 datasource 的查询权限。

## 定时 MCP 监控与提醒

在“系统设置”中填写 Prometheus 数据源 UID，配置检查间隔、重复提醒冷却时间和阈值，然后开启“MCP 定时监控”。应用运行期间会定时调用只读 `query_prometheus` 工具。

- 默认包含 CPU、内存、磁盘剩余空间和服务器在线状态四条规则。
- 支持连续多次超限后触发，避免瞬时毛刺；持续异常只在冷却期结束后重复提醒。
- 指标恢复后会发送恢复提醒。状态和最近 100 条事件存储于 SQLite。
- “立即检查”可验证数据源 UID、MCP 返回格式和全部规则。
- 应用重启后会从系统 Keychain 读取 Token，可继续执行定时监控。

## 项目结构

```text
src/app/                         应用入口、Provider 和顶层组合
src/features/workspace/          当前工作台功能组合（后续按页面继续拆分）
src/domain/report/               与框架无关的日报领域模型
src/domain/settings/             默认连接、调度和告警规则配置
src/infrastructure/tauri/        前端到 Tauri 的端口适配器
src-tauri/src/mcp/               MCP 协议、进程与官方服务客户端
src-tauri/src/lib.rs             Tauri 命令、SQLite 和配置组合根
src-tauri/capabilities/          Tauri 最小权限声明
```

## 下一阶段

1. 在现有 MCP `call_tool` 基础上实现 Report Collector，映射 `query_prometheus`、`query_loki_logs` 和 Alerting 工具。
2. 为 AI 分析增加结构化输出和结果持久化。
3. 增加查询审计、通知渠道和报告导出。
