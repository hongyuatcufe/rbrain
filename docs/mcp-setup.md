# rbrain MCP Server 配置指南

rbrain 的 MCP server 基于 [Model Context Protocol](https://modelcontextprotocol.io) 开放标准，可接入所有支持 MCP 的 AI 编码工具。

---

## 前置：确认 rbrain 已安装

```bash
which rbrain
rbrain --version

# 确认 brain 已初始化（在项目目录或 Home 下）
rbrain stats
```

rbrain 的 brain 自动发现规则：
- 优先找当前目录往上的 `.rbrain/` → 项目本地 brain
- 找不到则回退到 `~/.rbrain/` → 全局 brain

MCP server 被工具启动时，**CWD 通常是项目根目录**，因此项目本地 brain 会自动被找到。若想固定使用全局 brain，可在启动命令里加 `--brain-dir ~/.rbrain`（目前通过 `cd` 解决，见下文各节）。

---

## 工具 1：Claude Code

### 方法 A：命令行注册（推荐）

**用户级**（所有项目共用同一个 brain）：

```bash
claude mcp add --scope user rbrain -- /Users/hongyu/.cargo/bin/rbrain serve mcp
```

**项目级**（每个项目用自己的 `.rbrain/`）：

```bash
# 在项目根目录执行
claude mcp add --scope project rbrain -- /Users/hongyu/.cargo/bin/rbrain serve mcp
```

项目级会写入 `.mcp.json`，可提交到 git 让团队共享。

### 方法 B：手动编辑 settings.json

**用户级** → `~/.claude/settings.json`：

```json
{
  "mcpServers": {
    "rbrain": {
      "command": "/Users/hongyu/.cargo/bin/rbrain",
      "args": ["serve", "mcp"]
    }
  }
}
```

**项目级** → `.mcp.json`（项目根目录）：

```json
{
  "mcpServers": {
    "rbrain": {
      "command": "/Users/hongyu/.cargo/bin/rbrain",
      "args": ["serve", "mcp"]
    }
  }
}
```

### 验证

```bash
claude mcp list          # 应看到 rbrain
claude mcp get rbrain    # 查看详情和工具列表
```

---

## 工具 2：Codex CLI

### 方法 A：命令行注册

```bash
codex mcp add rbrain -- /Users/hongyu/.cargo/bin/rbrain serve mcp
```

若需指定工作目录（固定 brain 路径）：

```bash
codex mcp add rbrain --cwd /Users/hongyu/project/rbrain-test -- /Users/hongyu/.cargo/bin/rbrain serve mcp
```

### 方法 B：手动编辑 config.toml

**用户级** → `~/.codex/config.toml`：

```toml
[mcp_servers.rbrain]
command = "/Users/hongyu/.cargo/bin/rbrain"
args = ["serve", "mcp"]
```

**项目级** → `.codex/config.toml`（项目根目录）：

```toml
[mcp_servers.rbrain]
command = "/Users/hongyu/.cargo/bin/rbrain"
args = ["serve", "mcp"]
```

### 验证

```bash
codex mcp list           # 应看到 rbrain
```

在 Codex 会话里输入：

```
list the tools from rbrain mcp server
```

---

## 工具 3：OpenCode

### 方法 A：手动编辑 opencode.json

**用户级** → `~/.config/opencode/opencode.json`：

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "rbrain": {
      "type": "local",
      "command": ["/Users/hongyu/.cargo/bin/rbrain", "serve", "mcp"],
      "enabled": true
    }
  }
}
```

**项目级** → `opencode.json`（项目根目录）：

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "rbrain": {
      "type": "local",
      "command": ["/Users/hongyu/.cargo/bin/rbrain", "serve", "mcp"],
      "enabled": true
    }
  }
}
```

### 验证

启动 opencode，在会话里输入：

```
use brain_stats to show knowledge base stats
```

---

## 可用工具一览（17 个）

| 工具 | 功能 |
|------|------|
| `brain_query` | 混合搜索（向量 + BM25 + RRF），返回带 chunk_id 的结果 |
| `brain_get` | 按 slug 读取页面完整内容 |
| `brain_put` | 创建或更新页面 |
| `brain_delete` | 删除页面 |
| `brain_list` | 列出所有页面（支持 type/tag 过滤） |
| `brain_graph` | 知识图谱遍历（方向 + 深度） |
| `brain_backlinks` | 查谁链接到某页面 |
| `brain_outlinks` | 查某页面链接到哪里 |
| `brain_link` | 建立类型化链接（支持 chunk_id 锚定原文） |
| `brain_unlink` | 删除链接 |
| `brain_orphans` | 查找无入链的孤立页面 |
| `brain_stats` | 知识库统计信息 |
| `brain_generate` | 搜索 + DeepSeek LLM 合成 wiki 页（可选保存） |
| `brain_think` | 深度推理（核心观点/张力/工作判断/开放问题） |
| `brain_add_timeline_entry` | 给页面追加带日期的事件记录 |
| `brain_add_tag` | 给页面加标签 |
| `brain_remove_tag` | 移除标签 |

---

## 典型对话示例

配置好后，在任意工具的对话里可以直接用：

```
# 搜索
brain_query("孔子的教育思想", limit=5)

# 记录研究发现
brain_put(slug="concepts/you-jiao-wu-lei", content="...", page_type="concept")

# 锚定证据链接
brain_link(from="concepts/you-jiao-wu-lei", to="中国教育思想史", link_type="evidence", chunk_id=150)

# 深度推理
brain_think("有教无类与因材施教的内在张力", limit=10)

# 追加时间线
brain_add_timeline_entry(slug="figures/kong-qiu", text="提出有教无类", source="中国教育思想史 chunk:150")
```

---

## 注意事项

- **API Keys**：`brain_think` 和 `brain_generate` 需要 DeepSeek API key，`brain_query` 的向量搜索需要 Qwen embedding key。两者均配置在 `~/.rbrain/config.toml`。若无 key，`brain_query` 会降级到纯 BM25 keyword search。
- **并发限制**：rbrain 底层使用 SQLite，同一时刻只允许一个写操作。不要同时从多个工具对话向同一 brain 写入。
- **brain 路径**：每次工具启动 MCP server 时，CWD 决定哪个 brain 被找到。若需固定 brain，可在配置的 command 前加 `cd /your/brain/dir &&`（需用 shell 包裹，见下方）：

```json
{
  "command": "bash",
  "args": ["-c", "cd /Users/hongyu/project/rbrain-test && rbrain serve mcp"]
}
```

---

## 参考链接

- [Claude Code MCP 文档](https://code.claude.com/docs/en/mcp)
- [Codex CLI MCP 文档](https://developers.openai.com/codex/mcp)
- [OpenCode MCP 文档](https://opencode.ai/docs/mcp-servers/)
- [Model Context Protocol 规范](https://modelcontextprotocol.io)
