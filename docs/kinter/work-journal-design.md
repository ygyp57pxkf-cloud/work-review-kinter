# Work Journal 设计说明

## 定位与数据边界

Work Journal 是个人工作日志的本地优先功能：它从本机已记录的活动生成工作块（session）、按项目归因、允许人工复核，并可生成 Obsidian 日报预览。工作块、项目归因和导出记录分别保存在本地数据库的 `work_sessions`、`project_attributions`、`obsidian_exports` 表中；本功能不会因为生成日志而自动把数据提交到远端。

日志按用户选择的一天读取本地活动，并统一使用当前生效的 `PrivacyConfig` 生成安全副本。`Ignored` 活动完全排除；`Anonymized` 活动只保留应用名和时长；普通活动的窗口标题、OCR、规则证据、任务摘要、AI 提示词和 Obsidian 输出都会过滤邮箱、手机号、Token、API Key、Bearer 及常见令牌前缀。项目归因不会把完整 URL 路径或查询参数作为展示证据。

## Session 生成

1. 活动按起止时间排序，并按连续上下文聚合为 session。
2. 相邻活动间隔超过 5 分钟会拆分；连续且上下文类别发生持续跳转（至少 2 分钟）也会拆分。
3. 短暂的通讯活动可并入相邻的专注工作块；娱乐分类或“休息娱乐”语义不会与工作块混合。
4. session 的主应用是该块内累计时长最多的应用，活动 ID 列表保留为本地追溯依据。

## 项目归因与来源优先级

每个 session 最多保留一条项目归因。`source` 字段说明当前结果的来源，优先级和覆盖规则如下：

| 优先级 | `source` | 产生方式 | 重算行为 |
| --- | --- | --- | --- |
| 最高 | `manual` | 用户审阅后确认项目、摘要或状态 | `confirmed=true`，规则和 AI 均不能覆盖。 |
| 中 | `ai_vision` | 文本 AI 后按条件使用视觉模型得到的结果 | 后续规则重算不能覆盖未确认的 AI 结果。 |
| 中 | `ai_text` | 显式运行文本 AI 的结果 | 后续规则重算不能覆盖未确认的 AI 结果。 |
| 基础 | `rule` | 本地项目规则自动匹配 | 仅会覆盖旧的、未确认且同为 `rule` 的结果。 |

人工审阅是最终裁决：确认时置信度写为 100、`needs_review=false`、`confirmed=true`。人工还可将 session 标为 `private`（导出中以“私密工作”呈现且不带证据）或 `excluded`（不计入汇总和 Obsidian 导出）。

AI 结果使用数据库条件事务写入：只有当前归因仍是未确认的 `rule`、`ai_text` 或 `ai_vision` 时，才会在同一事务内更新项目归因和 session 摘要。若 AI 请求期间用户已经人工确认、设为私密或排除，本次结果会计入 `skipped_due_to_manual_review`，不会覆盖人工结果。历史重复归因迁移按 `confirmed`、`manual > ai_vision > ai_text > rule`、创建时间和 ID 确定保留项；被淘汰项先写入 `project_attributions_dedup_backup`，再在同一事务内删除。

规则从本机配置中的 `work_journal_project_rules` 读取。单条规则可用本地路径、域名、URL 关键词、窗口关键词、应用关键词和反向关键词匹配；规则优先级只在匹配分数相同时参与选择，并不把低证据直接伪装成高置信。娱乐活动不参与项目匹配。没有合适规则时结果是 `unassigned` / “待确认”，置信度为 0。

## 审阅与输出流程

1. 打开日期时，先由本地活动、session 和现有归因组装日视图。
2. 规则归因置信度低于 80，或未匹配项目时，标记为“待确认”。
3. 用户可直接人工确认、标私密或排除；也可在显式开启 AI 后只处理尚未确认的低置信 session。
4. Obsidian 先生成 Markdown 预览、SHA-256 和一次性确认 Token。默认模式不写文件；只有启用日报导出并提交仍有效的日期、hash 和 Token，才调用实际写入。

`source` 是可审计的当前归因来源，不等同于原始活动的采集来源。原始活动仍由 `activity_ids` 指向本地数据库记录；归因证据只保存经过截断和脱敏后的摘要。

## 旧 Work Review 数据导入

设置页提供显式的旧数据导入入口，默认定位系统的 `work-review` 数据目录，也允许用户手动选择。预检以只读方式打开 `workreview.db` 或 `work_review.db`，展示活动数量、日期范围、可复制截图数量、空间和跳过数；来源数据库和 WAL 在预检前后必须保持一致，确认 Token 有效期为 10 分钟。

正式导入前会在当前 Work Journal 数据目录的 `import-backups/` 备份现有数据库。导入范围只包含 `activities` 和来源目录内的普通截图文件，不包含旧配置、密钥、日报、AI 设置或 Work Journal 派生表。截图写入 `screenshots/imported/<run-id>/`；缺失、符号链接和越界路径会跳过并计数。活动内容指纹与 `work_journal_imported_activities` 映射表共同保证重复执行幂等；数据库事务失败时，本次截图目录会删除。
