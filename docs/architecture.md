# Architecture

`grafana_watch_dog` 采用 Ports and Adapters 风格，UI、领域数据、Tauri IPC 和外部 MCP 进程之间没有循环依赖。

```text
src/app (providers and composition)
  ↓ uses
src/features/workspace → src/domain ← src/infrastructure/tauri
                     ↓ invoke
src-tauri/lib (composition root)
  ├─ storage (SQLite)
  ├─ report pipeline
  └─ mcp/GrafanaMcpClient
          ↓ JSON-RPC over stdio
     official mcp-grafana
          ↓
     Grafana / Prometheus / Loki / Alerting
```

## Extension rules

- 新页面放在 `src/features/<feature>`，只通过 domain 类型和 infrastructure ports 获取数据。
- 跨功能 UI 放在 `src/shared/components`；业务组件留在所属 feature。
- Rust 的外部系统集成都放在独立模块；`lib.rs` 只负责 Tauri 命令注册和状态组合。
- MCP 工具名及参数映射应集中在 report collector，页面不得直接拼 PromQL/LogQL。
- 报告生成始终输出稳定的 `Report` JSON，避免模型输出直接驱动 UI。

## MCP lifecycle

1. 使用配置的命令启动官方 `mcp-grafana` 子进程。
2. 通过环境变量注入 Grafana URL 与 Service Account Token。
3. 发送 `initialize`，收到响应后发送 `notifications/initialized`。
4. 使用 `tools/list` 进行连接测试和能力发现。
5. 使用 `tools/call` 执行只读查询；client drop 时终止并回收子进程。

默认参数固定包含 `--disable-write`。即便如此，生产环境仍应给 Service Account 配置最小 RBAC 权限。
