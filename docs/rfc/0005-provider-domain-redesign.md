---
rfc: 0005
name: provider-domain-redesign
title: Provider Domain Redesign
description: Separates adapter kind, provider account, model reference, credential reference, and resolved model target to keep provider growth from polluting runtime and session semantics.
status: Draft
date: 2026-03-31
authors:
  - aia
supersedes: []
superseded_by: null
---

# RFC 0005: Provider Domain Redesign

## Summary

为 `aia` 重设计当前的 `provider` 领域模型，目标不是“立刻支持很多新 provider”，而是先把现有概念拆干净，避免后续每接一个新模型家族就把 `provider-registry`、`session-tape`、`agent-store` 和 `apps/agent-server` 一起拖着变形。

本 RFC 采纳以下核心方向：

1. 将当前 `ProviderKind` 明确收口为 **协议适配类型**，建议重命名为 `AdapterKind`
2. 将当前 `ProviderProfile` 明确收口为 **上游账户 / 端点配置**，建议演进为 `ProviderAccount`
3. 引入稳定的 `ProviderId` / `ModelRef`，让 session、Web、runtime 围绕“模型引用”而不是“临时拼出来的 provider + model + base_url + protocol”工作
4. 将认证语义从 provider 主体中拆出，至少先抽成 `CredentialRef` 概念，避免 `api_key` 长期作为 provider 本体字段蔓延
5. 让 runtime / adapter 最终只消费 `ResolvedModelTarget`，而不是直接拿 `ProviderProfile` 做协议分发

这个设计方向参考了当前几类 agent / coding harness 的共同模式：

- provider 负责“接入源 / 账户 / 上游目录”
- model 负责“可选目标与能力描述”
- auth 负责“如何拿到凭证”
- runtime 最终只关心“已经解析好的目标”

对 `aia` 来说，最关键的不是跟随某个外部项目命名，而是把下面这些概念彻底分开：

- **adapter**：协议族 / API 形态
- **account**：一个可连接的上游入口
- **model**：上游入口下的具体模型
- **credential**：认证材料或其引用
- **binding**：session 当前选中了哪个模型
- **resolved target**：runtime 最终实际要连的目标

## Implementation Snapshot

截至当前代码，provider 相关事实如下：

1. `crates/provider-registry/src/model.rs` 当前以 `ProviderKind + ProviderProfile + ModelConfig` 为主结构，其中：
   - `ProviderKind` 实际承载的是协议适配类型，目前只有 `OpenAiResponses` / `OpenAiChatCompletions`
   - `ProviderProfile` 同时承载名字、协议、base URL、API key、models
2. `crates/agent-store/src/provider.rs` 将 provider 落到 SQLite 的 `providers` / `provider_models` 两张表，其中 `api_key` 仍是 provider 表字段
3. `crates/session-tape/src/binding.rs` 的 `SessionProviderBinding::Provider` 仍直接保存：
   - `name`
   - `model`
   - `base_url`
   - `protocol`
   - `reasoning_effort`
4. `apps/agent-server/src/model/mod.rs` 当前通过 `ProviderLaunchChoice::OpenAi { profile, model, reasoning_effort }` 直接把 `ProviderProfile` 喂给 OpenAI 相关 adapter
5. `apps/agent-server/src/routes/provider/handlers.rs` 与相关 DTO 直接将 `kind` 作为 API 可见字段，并按 `protocol_name` 做解析

这意味着当前实现里至少混在一起了四层概念：

- provider 身份
- adapter 协议族
- auth 凭证
- model 目录与能力

RFC 的正文描述的是 **目标结构**，并不代表这些类型已经落地。当前代码真相优先看：

- `crates/provider-registry/src/model.rs`
- `crates/agent-store/src/provider.rs`
- `crates/session-tape/src/binding.rs`
- `apps/agent-server/src/model/mod.rs`
- `apps/agent-server/src/session_manager/provider_sync.rs`

## Motivation

### 1. 当前 `ProviderProfile` 的语义过满

当前 `ProviderProfile` 同时承担：

- 本地标识
- 用户可见名称
- 协议选择
- 认证方式
- endpoint 定义
- model 列表与能力

这让它在代码中看起来像“方便的配置对象”，但实际上已经跨越了多个边界。

短期内这样做很省事；中期开始就会出现两个问题：

1. 一旦引入第二个真实 provider 家族，`ProviderProfile` 很容易继续长字段
2. session / runtime / store 都会开始直接依赖这个胖对象，后面想拆会越来越痛

### 2. `ProviderKind` 其实不是 provider，而是 adapter

当前 `ProviderKind::OpenAiResponses` / `OpenAiChatCompletions` 描述的不是“上游供应商是谁”，而是“请求该怎么发”。

这点在接 OpenAI-compatible endpoint 时会特别明显：

- OpenAI 官方
- OpenRouter
- 兼容 OpenAI 的本地网关
- 自建兼容层

它们可能都走同一个 adapter，但绝不是同一个 provider identity。

如果继续把 adapter 叫作 provider kind，后面所有扩展都会越来越绕。

### 3. session tape 不应长期保存 endpoint 细节

当前 `SessionProviderBinding::Provider` 里直接持有 `base_url` 和 `protocol`。这会带来几个问题：

- session 绑定里混进了本应由 registry 解析的连接细节
- provider rename、endpoint 调整、auth 轮换时，session 事实容易和当前 registry 漂移
- 同一个 provider 名下若未来支持 credential 切换、label 改名、catalog 重排，会让 tape 恢复语义变得脆弱

更合理的做法是：

- tape 只记录稳定选择结果，例如 `ModelRef`
- 真正连接需要的 endpoint / adapter / credential 在运行时解析

### 4. `name` 同时承担稳定 ID 和显示名，后续会卡住

当前 `ProviderProfile.name` 同时承担：

- 本地主键
- session 绑定引用目标
- API 路径参数
- UI 展示名称

这意味着未来如果用户想“改个显示名”，系统实际上会面对“改主键”问题。

对 provider 这种会被 session 持久引用的对象，`id` 和 `label` 最好尽早分开：

- `id`：稳定、机器可依赖
- `label`：人类可读、可改名

### 5. `api_key` 不是 provider 本体字段

从领域语义看，认证不是 provider 身份本身的一部分。

同一个 provider account 未来完全可能出现：

- API key 轮换
- 环境变量注入
- OAuth 账户认证
- 测试 / CI 用不同 credential

如果 `api_key` 长期固化在 provider 主体里，那么后续所有 auth 演进都要反向污染 provider 主结构。

### 6. runtime 最终关心的是“解析后的目标”，不是 registry 原始对象

`openai-adapter`、未来的其他 adapter，以及 `apps/agent-server/src/model/mod.rs` 真正需要的，不是一个“胖 provider profile”，而是一份已经准备好的连接目标：

- 用哪个 adapter
- 连哪个 base URL
- 用什么 credential
- model id 是什么
- 这个 model 支持什么能力和默认值

所以 runtime 更适合接收 `ResolvedModelTarget` 这类结构，而不是直接消费 registry 原始数据。

## Goals

- 把 adapter、provider identity、model、credential、session binding 几层概念显式拆开
- 让 session 绑定围绕稳定 `ModelRef`，不再长期保存 `base_url` / `protocol`
- 让 provider rename 不再等价于 session 绑定失效
- 让 runtime 只消费解析后的 `ResolvedModelTarget`
- 保持 `provider-registry` 作为 provider 领域层，不把协议细节继续扩散回核心层
- 为未来接更多 provider 家族预留正确结构，但不要求本 RFC 同步完成大规模扩展
- 保持向后兼容，允许通过分阶段迁移平滑替换旧类型和旧落盘格式

## Non-Goals

- 本 RFC 不要求立即引入大量新的 provider 家族
- 本 RFC 不要求立刻接入远程模型目录自动发现
- 本 RFC 不要求一次性替换所有 Web 设置页交互形态
- 本 RFC 不要求当前阶段立刻落成完整 secret manager
- 本 RFC 不改变 `agent-core` 的工具协议或 session tape append-only 原则
- 本 RFC 不要求把所有 provider / credential 数据立即从 SQLite 迁出

## Proposal

### 1. 明确新的概念边界

本 RFC 建议在 provider 相关代码中显式区分以下概念：

#### 1.1 `AdapterKind`

表示“请求该怎么发”的协议适配类型。

建议以当前 `ProviderKind` 为基础重命名：

```rust
pub enum AdapterKind {
    OpenAiResponses,
    OpenAiChatCompletions,
    // 未来如有需要：AnthropicMessages, GeminiGenerateContent, ...
}
```

`AdapterKind` 的职责：

- 标识协议族
- 驱动 adapter 选择
- 定义 payload / streaming / error mapping 差异

它**不负责**：

- 表达上游账户身份
- 充当用户可见 provider 名称
- 存储认证材料

#### 1.2 `ProviderAccount`

表示一个“可连接的上游入口”。

建议 `ProviderProfile` 演进到如下语义：

```rust
pub struct ProviderAccount {
    pub id: String,
    pub label: String,
    pub adapter: AdapterKind,
    pub endpoint: ProviderEndpoint,
    pub credential: CredentialRef,
    pub models: Vec<ModelConfig>,
}

pub struct ProviderEndpoint {
    pub base_url: String,
}
```

语义约束：

- `id`：稳定主键，供 session / store / API 引用
- `label`：用户可见显示名，可允许未来改名
- `adapter`：连接协议，不等于 provider 身份
- `endpoint`：连接入口
- `credential`：认证引用，而不是原始 secret 本身
- `models`：此 account 当前暴露的模型目录

在当前阶段，`models: Vec<ModelConfig>` 仍然可以沿用现有结构，避免过早再拆出第二层 catalog 模型。

#### 1.3 `ModelRef`

表示一个稳定的模型引用。

建议新增：

```rust
pub struct ModelRef {
    pub provider_id: String,
    pub model_id: String,
}
```

或等价的稳定序列化形式，如 `provider_id/model_id`。当前更推荐结构化对象，原因是：

- tape / API 更明确
- 后续扩字段时兼容性更好
- 避免路径分隔符编码问题

`ModelRef` 是本 RFC 的关键收口点：

- Web settings 选中的是它
- session tape 记录的是它
- runtime 恢复时解析的是它
- adapter 最终连接的目标由它解析而来

#### 1.4 `CredentialRef`

表示认证材料的引用，而不是直接把 secret 放在 provider 实体上。

建议目标语义如下：

```rust
pub enum CredentialRef {
    Stored { id: String },
    EnvVar { name: String },
    OAuthAccount { id: String },
    None,
}
```

需要注意：

- 这只是 **目标语义**
- 现阶段可以先通过兼容层，把现有 SQLite 里的 `api_key` 视为 migration-only 的旧来源
- 也就是说，先把“credential 是一个独立概念”立住，再逐步把真实 secret 材料迁出 provider 主结构

#### 1.5 `ResolvedModelTarget`

表示 runtime 最终真正要用的模型目标。

建议新增类似结构：

```rust
pub struct ResolvedModelTarget {
    pub model_ref: ModelRef,
    pub adapter: AdapterKind,
    pub base_url: String,
    pub credential: ResolvedCredential,
    pub model: ModelConfig,
}

pub enum ResolvedCredential {
    ApiKey(String),
    OAuthBearer(String),
    None,
}
```

其中：

- `provider-registry` 最多负责解析到 `CredentialRef`
- 拿到真实 secret 的最后一步，可由 `apps/agent-server` 当前的模型构建层承担
- `openai-adapter` 和未来的其他 adapter 只接收已经完成解析的 target

### 2. session binding 改为围绕 `ModelRef`

当前 `SessionProviderBinding::Provider` 直接存 `name + model + base_url + protocol + reasoning_effort`，建议演进为：

```rust
pub enum SessionModelBinding {
    Bootstrap,
    Model {
        model_ref: ModelRef,
        reasoning_effort: Option<String>,
    },
}
```

若保留当前名字，也至少建议将 `SessionProviderBinding::Provider` 的内容收缩为：

```rust
Provider {
    model_ref: ModelRef,
    reasoning_effort: Option<String>,
}
```

这里的关键不是名字，而是语义：

- tape 只记录“用户选了哪个模型”
- 连接细节一律回到 registry / resolver 去解析

这样能带来两个直接收益：

1. provider endpoint / credential 调整后，历史 session 不必重写 tape
2. session 恢复逻辑不再依赖历史 tape 里残留的旧 base URL / protocol

### 3. `name` 拆为 `id + label`

当前 `ProviderProfile.name` 建议演进为：

- `id`：稳定 ID，默认可由现有 `name` 迁移而来
- `label`：用户可见展示名，首次迁移时可默认等于旧 `name`

推荐约束：

- `id` 一旦创建后不鼓励修改
- API 路径参数、session 绑定、SQLite 外键都应逐步切到 `id`
- UI 列表与设置页展示优先用 `label`

这样未来即便用户改显示名，也不会影响：

- session binding
- store 关联
- trace 归因

### 4. `ModelConfig` 继续保留为模型能力描述

本 RFC 不建议在当前阶段继续把模型层拆成更复杂的全球 catalog / provider override 两级系统。

当前更稳的方案是：

- 保留现有 `ModelConfig`
- 将其语义明确为“某 `ProviderAccount` 下可选模型的能力与默认值描述”

即：

```rust
pub struct ModelConfig {
    pub id: String,
    pub display_name: Option<String>,
    pub limit: Option<ModelLimit>,
    pub default_temperature: Option<f32>,
    pub supports_reasoning: bool,
}
```

若未来确实出现“同一模型目录需要被多个 provider 复用”的需求，再进一步拆出全局 `ModelDescriptor`；当前阶段先不把结构做重。

### 5. provider registry 提供“解析”能力，而不是只做列表容器

当前 `ProviderRegistry` 主要是增删查和 `first_provider()`。本 RFC 建议它逐步演进出以下能力：

```rust
impl ProviderRegistry {
    pub fn provider(&self, id: &str) -> Option<&ProviderAccount>;
    pub fn resolve_model(&self, model_ref: &ModelRef) -> Result<ResolvedModelSpec, ProviderRegistryError>;
    pub fn first_model_ref(&self) -> Option<ModelRef>;
}

pub struct ResolvedModelSpec {
    pub provider_id: String,
    pub adapter: AdapterKind,
    pub base_url: String,
    pub credential: CredentialRef,
    pub model: ModelConfig,
}
```

其中 `ResolvedModelSpec` 仍不需要携带真实 secret，只需要：

- provider 领域侧能稳定产出的解析结果
- 足够让 server 继续完成最后一步 credential resolution

这样 `provider-registry` 就从“配置列表容器”进化为真正的 provider 领域模型层。

### 6. `apps/agent-server` 只做最后一段解析和装配

当前 `apps/agent-server/src/model/mod.rs` 直接围着 `ProviderProfile` 做分发。建议演进为：

1. session manager 根据当前 `SessionModelBinding` 取到 `ModelRef`
2. `ProviderRegistry` 解析出 `ResolvedModelSpec`
3. server 侧 `CredentialResolver` 将 `CredentialRef -> ResolvedCredential`
4. 得到 `ResolvedModelTarget`
5. `build_model_from_selection(...)` 演进为 `build_model_from_target(...)`

示意：

```rust
pub fn build_model_from_target(
    target: ResolvedModelTarget,
    trace_store: Option<Arc<AiaStore>>,
) -> Result<(ModelIdentity, ServerModel), ServerSetupError>
```

这样 `apps/agent-server` 的职责会更清楚：

- 它负责 control-plane 和装配
- 但不再长期拥有 provider 领域对象的主语义

### 7. Web / HTTP API 围绕 provider account 与 model ref 暴露

当前 `/api/providers` API 直接暴露 `kind`, `base_url`, `api_key`, `models` 这类 profile 形态。本 RFC 建议长期演进为：

#### 7.1 Provider list item

```json
{
  "id": "main",
  "label": "OpenAI Main",
  "adapter": "openai-responses",
  "base_url": "https://api.openai.com/v1",
  "credential": {
    "type": "stored",
    "configured": true
  },
  "models": [
    {
      "id": "gpt-5",
      "display_name": "GPT-5",
      "supports_reasoning": true
    }
  ]
}
```

注意这里的 `credential` 应该是 **配置状态**，不是 secret 回显。

#### 7.2 Session settings

session settings 建议围绕：

```json
{
  "model_ref": {
    "provider_id": "main",
    "model_id": "gpt-5"
  },
  "reasoning_effort": "medium"
}
```

而不是继续传：

- provider name
- model string
- protocol
- base_url

### 8. SQLite 迁移应分阶段进行，而不是一口气翻表

当前 `agent-store` 已有：

- `providers`
- `provider_models`

本 RFC 不要求第一步就把表结构彻底推倒重来。更稳的路线是：

#### 阶段 A：先改语义，不急着改物理表名

- Rust 侧引入 `AdapterKind` / `ProviderAccount` / `ModelRef`
- SQLite 仍可沿用现有表
- `providers.name` 先视为迁移期的 `provider_id`
- 新增 `label` 列时，默认回填为当前 `name`

#### 阶段 B：补出 credential 语义

- 在 store 增加 `credential_kind` / `credential_ref` 或独立 credential 表
- 旧 `api_key` 走兼容读路径
- 逐步把 API / Web 入口从直接传 `api_key` 切到 credential 配置模型

#### 阶段 C：清理旧字段与旧兼容逻辑

- session tape 迁移为只保存 `ModelRef`
- provider routes 不再接受旧 `kind + api_key` 直写风格
- 清理旧 `ProviderProfile` / `ProviderKind` 命名

## Alternatives Considered

### 1. 继续沿用 `ProviderProfile`，只是在上面加字段

即继续在当前结构上追加：

- `display_name`
- `auth_type`
- `oauth_account_id`
- `default_model`
- `supports_vision`
- 更多 provider kind

未采纳原因：

- 这是把领域分层问题继续往后拖
- 短期最省事，长期最难拆
- session、store、Web、runtime 会更深地绑定这个胖对象

### 2. 让 `openai-adapter` 或 server model builder 直接承担全部 provider 语义

这种方案会把 provider 选择、credential 解析、model 目录判断都继续压在 `apps/agent-server/src/model/mod.rs` 一类装配层里。

未采纳原因：

- 违反当前架构里 shared core / edge adapter / app 壳的边界
- 会让 `agent-server` 继续长成 provider 巨石入口
- 未来接更多 provider 家族时耦合会更重

### 3. session tape 继续保存 `base_url` / `protocol`

这种方案的好处是恢复时似乎“更自给自足”。

未采纳原因：

- 会让历史 tape 固化过时连接细节
- provider registry 调整后，session 恢复容易漂移
- 让 append-only 事实混入本应可替换的配置投影

### 4. 一开始就引入完整全球 model catalog

例如：

- 全局 `ModelDescriptor`
- provider 只持有引用
- 各 provider 再覆盖差异项

未采纳原因：

- 当前阶段还没有真实需求证明这层复杂度值得
- 会让本轮 redesign 过大
- 与“先把概念拆开，再逐步扩”的目标不一致

## Risks and Mitigations

### 1. 风险：抽象过度，超出当前阶段实际需求

缓解：

- 当前只引入最关键的 4 个收口点：`AdapterKind`、`ProviderAccount`、`ModelRef`、`ResolvedModelTarget`
- `ModelConfig` 暂不继续细拆
- 不把“支持很多 provider”当作本 RFC 的验收线

### 2. 风险：迁移范围横跨 store、tape、server、Web

缓解：

- 按阶段 rollout
- 先引入新类型，再加兼容层
- 旧 JSON / SQLite 读路径保留一段时间

### 3. 风险：`CredentialRef` 与实际 secret 存储边界不清

缓解：

- 先把“引用”和“实际 secret”概念拆开
- 第一阶段允许 store 兼容旧 `api_key` 列
- 但 public API 和新领域类型不再把 secret 视为 provider 本体字段

### 4. 风险：`provider` / `account` 命名切换导致理解成本上升

缓解：

- HTTP 路由短期仍可保留 `/api/providers`
- 文档中明确：这里的 provider 指的是 `ProviderAccount`
- 内部代码优先切到清晰命名，外部路径兼容保守演进

### 5. 风险：现有 session 因 provider rename 或 migration 出现恢复问题

缓解：

- 第一阶段迁移时将旧 `name` 直接提升为稳定 `provider_id`
- `label` 默认复制旧 `name`
- session tape 旧 binding 通过兼容反序列化转换为新 `ModelRef`

## Open Questions

- `ModelRef` 的公开序列化形式最终采用结构化对象还是单字符串更合适？
- `CredentialRef` 的第一版应直接落独立表，还是先用兼容字段承接？
- 是否要在 `ProviderAccount` 上引入显式 `default_model_id`，还是继续沿用“列表第一个就是默认”语义？
- `ProviderRegistry` 是否应该同时负责解析 provider label / display projection，还是只负责领域解析？
- 未来若同一上游账户下出现多个 endpoint scope，是否需要再拆 `ProviderAccount` 与 `EndpointProfile`？

## Rollout Plan

### Phase 1: 先把概念和命名拆开

1. 在 `provider-registry` 引入：
   - `AdapterKind`
   - `ProviderAccount`
   - `ModelRef`
2. 保留旧 `ProviderKind` / `ProviderProfile` 的兼容转换
3. 在 `apps/agent-server` 增加从旧 selection 到 `ResolvedModelSpec` 的桥接层

预期结果：

- 新旧类型可并行存在
- 行为不变
- 代码中的概念边界先立住

### Phase 2: session binding 切到 `ModelRef`

1. 为 `session-tape` 增加新 binding 结构
2. 保留旧 `name + model + protocol + base_url` 反序列化兼容
3. 恢复路径优先以 `ModelRef` 解析 provider 选择

预期结果：

- 新 session 不再写连接细节到 tape
- 老 session 仍能恢复

### Phase 3: runtime 构建切到 `ResolvedModelTarget`

1. `ProviderRegistry` 增加 `resolve_model(...)`
2. `apps/agent-server/src/model/mod.rs` 将 `build_model_from_selection(...)` 演进为 `build_model_from_target(...)`
3. OpenAI 相关 adapter 改为接收解析后的 target

预期结果：

- model builder 不再依赖胖 provider 对象
- adapter 边界更清楚

### Phase 4: credential 语义独立化

1. 引入 `CredentialRef`
2. store 增加 credential 配置字段或表
3. Web / API 改为展示 credential 配置状态，而不是回显 secret 字段

预期结果：

- secret 不再作为 provider 本体字段扩散
- 为后续 env / oauth / key rotation 留出口

### Phase 5: 清理旧命名与旧兼容层

1. 清理 `ProviderKind` / `ProviderProfile` 旧入口
2. 清理旧 session binding 写入格式
3. 清理直接基于 `protocol_name` 的 API 主路径语义

预期结果：

- provider 设计彻底切换到新模型
- 新增 provider 家族时，不再需要改动 session binding 语义

## Success Criteria

- session tape 中的新 provider 绑定不再保存 `base_url` / `protocol`
- provider rename 只影响显示层，不影响既有 session 恢复
- runtime / adapter 只依赖 `ResolvedModelTarget` 一类解析结果，而不是 `ProviderProfile`
- `ProviderKind` 与 provider identity 的概念不再混淆
- `api_key` 不再作为长期公开领域类型中的 provider 本体字段
- 后续新增一个新的 adapter family 时，不需要再次重写 session binding 与 provider store 基础语义

## Appendix A: 旧类型到新类型的对照草图

这一节不是最终 API 定稿，而是为了帮助本仓库逐步迁移时，对照“当前代码里的谁，未来应该收口成谁”。

### A.1 `provider-registry`

| 当前类型 | 位置 | 当前语义 | 建议目标类型 | 迁移说明 |
|---|---|---|---|---|
| `ProviderKind` | `crates/provider-registry/src/model.rs` | 实际是协议适配类型 | `AdapterKind` | 第一阶段先重命名并补兼容别名 |
| `ProviderProfile` | `crates/provider-registry/src/model.rs` | provider + endpoint + auth + models 混合体 | `ProviderAccount` | 第一阶段保留字段，但补出 `id` / `label` / `credential` 语义 |
| `ModelConfig` | `crates/provider-registry/src/model.rs` | 某 provider 下的模型能力 | `ModelConfig` | 先保留，不急着拆成更复杂 catalog |
| `ProviderRegistry` | `crates/provider-registry/src/registry.rs` | provider 列表容器 | `ProviderRegistry` | 保留名字，但增加 `provider()` / `resolve_model()` 能力 |

### A.2 `session-tape`

| 当前类型 | 位置 | 当前语义 | 建议目标类型 | 迁移说明 |
|---|---|---|---|---|
| `SessionProviderBinding::Provider { name, model, base_url, protocol, reasoning_effort }` | `crates/session-tape/src/binding.rs` | 模型选择 + 连接细节一起保存 | `SessionModelBinding::Model { model_ref, reasoning_effort }` | 第一阶段新增兼容反序列化，写路径先切新格式 |
| `name` | 同上 | 稳定 provider 标识 + 显示名 | `provider_id` | 从旧 `name` 直接迁移 |
| `model` | 同上 | model id | `model_ref.model_id` | 直接迁移 |
| `base_url` / `protocol` | 同上 | 连接细节 | 无 | 不再进入新 tape |

### A.3 `agent-store`

| 当前类型/表 | 位置 | 当前语义 | 建议目标 | 迁移说明 |
|---|---|---|---|---|
| `StoredProviderProfile` | `crates/agent-store/src/provider.rs` | provider 持久化投影 | `StoredProviderAccount` | 第一阶段可只改 Rust 类型名，不急着改表名 |
| `providers.name` | SQLite | 主键 + 显示名 | `provider_id` + `label` | 第一阶段先新增 `label` 列 |
| `providers.kind` | SQLite | 实际 adapter kind | `adapter` | 先保持旧列名兼容 |
| `providers.api_key` | SQLite | secret 材料 | `credential_*` | 先兼容读旧列，后续再迁 |
| `provider_models` | SQLite | provider 下模型能力 | `provider_models` | 先不动 |

### A.4 `apps/agent-server`

| 当前类型 | 位置 | 当前语义 | 建议目标类型 | 迁移说明 |
|---|---|---|---|---|
| `ProviderLaunchChoice::OpenAi { profile, model, reasoning_effort }` | `apps/agent-server/src/model/mod.rs` | runtime 选模时直接携带整个 provider profile | `ResolvedModelTarget` | 第一阶段可先新增枚举分支或桥接构造函数 |
| `build_model_from_selection(...)` | 同上 | 直接消费 registry 原始结构 | `build_model_from_target(...)` | 第二阶段正式切换 |
| `parse_provider_kind(...)` | `apps/agent-server/src/routes/provider/handlers.rs` | API 级协议字符串解析 | `parse_adapter_kind(...)` | 第一阶段只改内部命名 |
| `CreateProviderInput` / `UpdateProviderInput` | `apps/agent-server/src/runtime_worker/*` | 直接围着旧 profile 入参工作 | `CreateProviderAccountInput` / `UpdateProviderAccountInput` | 可以等 API 形态稳定后再切名 |

## Appendix B: 建议的新类型草图

这部分是“贴着当前代码可落地”的一版草图，故意不做太重。

### B.1 `provider-registry`

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AdapterKind {
    OpenAiResponses,
    OpenAiChatCompletions,
}

impl AdapterKind {
    pub fn protocol_name(&self) -> &'static str {
        match self {
            Self::OpenAiResponses => "openai-responses",
            Self::OpenAiChatCompletions => "openai-chat-completions",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderEndpoint {
    pub base_url: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CredentialRef {
    LegacyApiKey,
    Stored { id: String },
    EnvVar { name: String },
    None,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModelRef {
    pub provider_id: String,
    pub model_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProviderAccount {
    pub id: String,
    pub label: String,
    pub adapter: AdapterKind,
    pub endpoint: ProviderEndpoint,
    pub credential: CredentialRef,
    pub models: Vec<ModelConfig>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedModelSpec {
    pub model_ref: ModelRef,
    pub adapter: AdapterKind,
    pub base_url: String,
    pub credential: CredentialRef,
    pub model: ModelConfig,
}
```

这里有个刻意设计：`CredentialRef::LegacyApiKey`。

原因不是因为它优雅，而是因为它能帮迁移阶段少折腾：

- 旧 store 里已经有 `api_key`
- 第一阶段不必立刻建 credential 新表
- 只要 resolver 知道“这个 provider 仍从旧 `api_key` 列取 secret”就行

这样能先把 **provider 本体语义** 和 **secret 存储细节** 分开。

### B.2 `session-tape`

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SessionProviderBinding {
    Bootstrap,
    Provider {
        model_ref: ModelRef,
        #[serde(default)]
        reasoning_effort: Option<String>,
    },
}
```

兼容旧格式时，推荐在反序列化层做桥接：

```rust
#[derive(Deserialize)]
#[serde(untagged)]
enum SessionProviderBindingWire {
    New {
        model_ref: ModelRef,
        #[serde(default)]
        reasoning_effort: Option<String>,
    },
    Legacy {
        name: String,
        model: String,
        #[serde(default)]
        base_url: String,
        #[serde(default = "default_provider_protocol")]
        protocol: String,
        #[serde(default)]
        reasoning_effort: Option<String>,
    },
}
```

然后把旧 `Legacy { name, model, .. }` 转成：

```rust
SessionProviderBinding::Provider {
    model_ref: ModelRef {
        provider_id: name,
        model_id: model,
    },
    reasoning_effort,
}
```

注意：

- 旧 `base_url` / `protocol` 仍然读，但只用于兼容
- 一旦保存回新 tape，就不再写它们

### B.3 `apps/agent-server/src/model/mod.rs`

```rust
pub struct ResolvedModelTarget {
    pub model_ref: ModelRef,
    pub adapter: AdapterKind,
    pub base_url: String,
    pub credential: ResolvedCredential,
    pub model: ModelConfig,
    pub reasoning_effort: Option<ReasoningEffort>,
}

pub enum ResolvedCredential {
    ApiKey(String),
    None,
}
```

第一阶段甚至可以先只支持：

- `ResolvedCredential::ApiKey(String)`
- `ResolvedCredential::None`

不要一开始就把 OAuth、account auth、token refresh 都塞进来。

## Appendix C: 第一阶段最小改造顺序

这部分是我觉得最稳的动手顺序。重点是：**先把概念立住，行为尽量不变。**

### Step 1: 在 `provider-registry` 里先引入新名字和新壳

建议先动：

- `crates/provider-registry/src/model.rs`
- `crates/provider-registry/src/lib.rs`
- `crates/provider-registry/tests/lib/mod.rs`

具体做法：

1. 新增 `AdapterKind`，保留 `ProviderKind` 兼容 alias 或转换
2. 新增 `ModelRef`
3. 新增 `ProviderAccount`，字段先尽量复用 `ProviderProfile`
4. `ProviderRegistry` 先增加：
   - `provider(&self, id: &str)`
   - `resolve_model(&self, model_ref: &ModelRef)`

第一阶段不要同时：

- 改 store schema
- 改 session tape 写入
- 改 web API

因为那样变更面太大。

### Step 2: 给 `agent-server` 加一个桥接解析层

建议先动：

- `apps/agent-server/src/model/mod.rs`
- `apps/agent-server/src/session_manager/provider_sync.rs`
- `apps/agent-server/src/session_manager/mod.rs`

具体做法：

1. 保留 `build_model_from_selection(...)`
2. 新增 `resolve_selection_to_target(...)`
3. 再新增 `build_model_from_target(...)`
4. 让旧路径内部先走：
   - 旧 selection
   - 解析成 target
   - 再建 model

这样以后改 session binding 的时候，model builder 那边已经提前收口了。

### Step 3: 切 session tape 的写路径

建议先动：

- `crates/session-tape/src/binding.rs`
- `crates/session-tape/tests/lib/mod.rs`
- `apps/agent-server/src/session_manager/provider_sync.rs`

具体做法：

1. 先让 `SessionProviderBinding` 支持新旧两种反序列化
2. 再让新的写路径只写 `ModelRef`
3. 恢复时优先走 `ModelRef`
4. 老 tape 依旧能读

这一步完成后，最讨厌的 `base_url` / `protocol` 漂移问题就基本被切掉了。

### Step 4: 最后再切 API 和 store 语义

建议再动：

- `crates/agent-store/src/provider.rs`
- `apps/agent-server/src/routes/provider/mod.rs`
- `apps/agent-server/src/routes/provider/handlers.rs`
- `apps/agent-server/tests/routes/provider/mod.rs`

具体做法：

1. 给 `providers` 表补 `label`
2. 把 API DTO 里的 `kind` 改内部语义为 `adapter`
3. 让 list API 返回 `id + label`
4. secret 改成“是否已配置”的投影，而不是公开字段

## Appendix D: 当前最值得先改的根因点

如果只挑一个最有价值的切口，我会先改这个：

### 把 `SessionProviderBinding` 从“连接细节快照”改成“模型引用”

原因很简单：

- 这条线正好穿过 session 恢复、runtime 重绑、provider 修改这些最容易漂移的地方
- 改完它之后，provider redesign 才真的从“文档”进入“事实”
- 而且它对 Web UI 的表层影响相对较小，主要是后端内部收口

对应当前热点文件就是：

- `crates/session-tape/src/binding.rs`
- `apps/agent-server/src/session_manager/provider_sync.rs`
- `apps/agent-server/src/model/mod.rs`

## Appendix E: 讨论建议

如果我们下一轮要把 RFC 往 `Accepted` 推，我建议优先把下面 3 个问题拍死：

1. `ModelRef` 是否正式采用 `{ provider_id, model_id }` 结构，而不是单字符串
2. `ProviderProfile -> ProviderAccount` 这次是否直接改名，还是先加新类型并做兼容
3. 第一阶段是否接受 `CredentialRef::LegacyApiKey` 这种迁移态设计

我自己的倾向是：

- `ModelRef`：用结构化对象
- `ProviderAccount`：先加新类型，旧类型做兼容转换，别硬改到满仓爆炸
- `CredentialRef::LegacyApiKey`：接受，先把边界分开，优雅以后再说

