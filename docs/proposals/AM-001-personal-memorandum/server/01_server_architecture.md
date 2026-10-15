# 服务端架构

## 1. 部署与模块边界

Go 模块化单体，一个 API/后台处理进程 + PostgreSQL + 持久化附件目录。TLS 可由 Caddy/Nginx 或已部署入口提供；首版不引入 Redis、Kafka、向量数据库或独立附件服务。附件存储抽象 BlobStore，未来可替换 S3 兼容存储，API 与领域动作不改变。

| 未来路径（相对 services/api/） | 模块职责 |
|---|---|
| cmd/memorandum/main.go | 启动、配置、优雅退出 |
| internal/auth/ | 注册码、设备凭据、scope、owner 校验 |
| internal/items/ | Item 聚合、动作、回顾、校验、撤销 |
| internal/attachments/ | 元数据、流上传、校验、下载、回收 |
| internal/reminders/ | rule/occurrence、到期处理、失效 |
| internal/sync/ | 用户锁、变更日志、游标、快照与 SSE |
| internal/history/ | 事件与可恢复版本 |
| internal/export/ | JSON/Markdown/图片导出任务 |
| internal/storage/postgres/ | 事务、SQL、迁移 |
| internal/http/ | 认证中间件、OpenAPI 校验、请求限制 |

HTTP handler 只转换协议、调用领域服务，不能自行更新 item_state。后台循环和未来 Agent 适配都复用同一领域服务；数据库实现依赖领域接口，领域层不依赖 HTTP。

## 2. 数据库约束与索引

[数据字典](../00_shared/01_domain_and_api.md)定义业务字段，建表至少包含下列约束：

```sql
-- 设计片段：不是已运行的迁移。
CHECK ((kind = 'task' AND task_state IS NOT NULL)
    OR (kind IN ('capture', 'memo') AND task_state IS NULL));
CHECK (due_at IS NULL OR due_date IS NULL);
CHECK (revision >= 1);
UNIQUE (owner_id, id);
-- group、attachment、rule 等使用含 owner_id 的复合外键，阻止跨用户引用。
-- PostgreSQL CHECK 对 NULL 可通过，完成状态约束须显式比较两侧布尔值。
CHECK ((task_state IS NOT DISTINCT FROM 'completed') = (completed_at IS NOT NULL));
CHECK ((task_state IS NOT DISTINCT FROM 'canceled') = (canceled_at IS NOT NULL));
```

标题和附件跨表非空校验在领域事务中执行，数据库不能用普通 CHECK 跨表查询来实现。pending 附件允许 Item 先同步，远端明确显示图片待上传；只有持有合法声明的原设备可补传，不接受未知 attachment_id。

| 表 | 关键索引 |
|---|---|
| items | (owner_id,deleted_at,kind,task_state)、(owner_id,planned_for)、(owner_id,review_at)、(owner_id,due_at)、(owner_id,last_reviewed_at,created_at,id) |
| item_events | (owner_id,item_id,occurred_at,id)；operation_id 关联 receipt |
| reminder_occurrences | 唯一(rule_id,schedule_revision,occurrence_at)；(status,occurrence_at) 部分索引 |
| attachments | (owner_id,item_id,position)、(state,updated_at) |
| change_log | 主键(owner_id,seq) |
| operation_receipts | 主键(owner_id,operation_id) |
| notification_deliveries | 唯一(device_id,operation_id)，(device_id,occurrence_id) |
| background_jobs | (status,next_attempt_at)，lease_until |

`updated_at` 不作为同步游标。所有查询从认证上下文绑定 owner_id；路径 id 不足以证明权限。无需全事件溯源，Item 当前状态用于查询，事件保存必要旧值/快照用于历史与撤销。

## 3. 提醒与工作领取

创建 rule 和 occurrence 同事务；工作循环每秒查到期候选，只更新 durable 状态。失败最多自动重试 8 次（1 秒起，最多 5 分钟，带抖动），超过后 failed + 错误摘要；管理命令可重新入队。失败不能让事项从关注计算里消失：关注仍可由有效规则与时间计算。

领取可使用 `FOR UPDATE SKIP LOCKED`，但所有实际业务写入锁顺序仍是 owner → Item/Rule。先用短事务领取工作并写 lease，释放工作表锁；再开启用户事务重新检查状态、版本、租约和调度是否有效。这样避免后台先锁 occurrence、用户动作先锁 owner 导致死锁。租约建议 30 秒，崩溃后过期重领，幂等失效操作不得生成第二实例。[PostgreSQL SELECT](https://www.postgresql.org/docs/current/sql-select.html)

首版服务端不向 Mac 发系统推送；SSE 只是提示同步。时间到达不依赖 Mac 在线；客户端展示仍按本地已知规则计算，不等待工作循环才认为已经到期。

## 4. 图片协议

metadata 同 Item 创建或 attach 动作保存，state=pending。之后上传：

1. `PUT /attachments/{id}/content` 携 Bearer、Idempotency-Key、预期 SHA-256 和字节长度。
2. 服务端确认 owner、附件未删除、类型和容量限额；流式写 staging，不把整图读入内存。
3. 校验实际字节、魔数、解码尺寸与像素数，防止仅扩展名通过；拒绝 SVG、HTML、动图和压缩包。
4. 计算 SHA-256，与声明一致才原子 rename 到 owner/attachment 专用不可变位置并持久化。
5. 用户串行事务再次验证关联未被删除，标记 ready、写 change_log 与 receipt。
6. 如果步骤 5 前进程崩溃，重试检查已经存在且哈希相同的文件，完成同一次状态提交；不同内容禁止覆盖。

文件系统和数据库没有分布式事务，所以必须有 staging、状态机和修复扫描，不能把两个写操作叫作一个原子事务。文件先到、DB 后到留下的无引用文件有 24 小时宽限再清理；pending 声明未上传只标失败，不自动删除用户 Item。附件下载只有 ready 可用，跨 owner 返回 404。

首版整文件重试，最大 20 MiB 可接受；字节级断点续传后置。并发上传默认每设备 2 个，总附件容量建议每账号 5 GiB，可配置；总容量以已存储+有效预留计，预留需过期与释放，避免并发绕限。拒绝上传不影响本地原图。

服务端不跨用户内容去重；哈希用于完整性。首版不自动删 EXIF 或重编码上传图片以免破坏原始内容；客户端自己生成的截图不写位置信息，导入原图可能保留其元数据，在隐私说明中列明。

## 5. 列表、搜索与历史

所有列表采用稳定键游标与 owner 过滤，默认 limit=50，最大 200；回顾候选返回 total 和 next_cursor。搜索首版使用有转义的字面子串，覆盖[客户端约定](../macos/01_client_architecture.md)相同字段；10,000 项先测基线，再按数据加 pg_trgm 等索引，不以英文分词漏掉中文短词。查询参数不得拼接到 SQL。

历史记录按 event id/时间稳定分页。撤销只对允许的动作开放：保存、修改、状态、删除、附件关联、改期；物理清理和设备撤销不支持通用撤销。反向操作必须检查当前 Item revision，并生成新的事件；撤销“完成”不自动恢复已过期的系统通知。

## 6. API 实现约束

JSON 请求设 1 MiB 上限，body_md 单独 256 KiB；普通请求超时建议 15 秒，上传 120 秒，流服务独立配置。错误返回稳定 code、request_id，不返回 SQL 或内部路径。URL 字符串不触发服务端抓取，避免把链接记录扩展成 SSRF 面。

健康检查分 liveness（进程）与 readiness（数据库、附件卷写入探测）；日志只记录 operation_id、结果、耗时、错误分类，不记录正文、原图、令牌或注册码。SSE 连接数与写入速度有限额，但不得以限流丢弃已保存操作；429 提供 Retry-After。

## 7. 验收与演进

必须通过 AC-SYNC-001 至 AC-SYNC-005、AC-ATT-001、AC-ATT-002、AC-AUTH-001、AC-OPS-001、AC-API-001，测试示例见[测试设计](../00_shared/06_test_design.md)。未来更换 BlobStore、增加 Agent scope、其他客户端，不改变 Item 结束语义和幂等协议。模块拆服务须有实际瓶颈证据，首版无该前提。
