# `ToolArgsSchema` 用法清单

本文件只描述当前自研 schema derive 的稳定用法。

## 支持的类型

- `String`
- `usize`
- `u32`
- `u64`
- `Option<String>`
- `Option<usize>`
- `Option<u32>`
- `Option<u64>`

其中：

- 普通字段会进入 `required`
- `Option<_>` 字段不会进入 `required`
- 整数字段会生成 `integer`，并带 `minimum: 0`

## 支持的结构形态

- 仅支持命名字段 `struct`
- 不支持泛型参数
- 不支持 `enum`
- 不支持 tuple struct / unit struct

## 支持的属性

### 容器级

```rust
#[tool_schema(min_properties = 1)]
```

当前只支持：

- `min_properties`

### 字段级

```rust
#[tool_schema(description = "字段说明")]
```

当前只支持：

- `description`

### `serde` 协作

当前会识别：

- `#[serde(rename = "...")]`

当前会显式拒绝：

- `#[serde(flatten)]`
- `#[serde(rename_all = "...")]`

`#[serde(deny_unknown_fields)]` 可以继续使用，但它影响的是反序列化行为，不额外改变 schema 生成结果。

## 示例

```rust
#[derive(Serialize, Deserialize, ToolArgsSchema)]
#[serde(deny_unknown_fields)]
#[tool_schema(min_properties = 1)]
struct ApplyPatchToolArgs {
    #[tool_schema(description = "The full patch text in apply_patch format")]
    patch: Option<String>,
    #[serde(rename = "patchText")]
    #[tool_schema(description = "Alias for patch; the full patch text in apply_patch format")]
    patch_text: Option<String>,
}
```

## 为什么编辑器里没有内部键提示

`tool_schema(...)` 是 derive helper attribute。

当前这类属性通常只能让编辑器知道“属性名合法”，但不会自动知道括号内部允许哪些键。因此 `description`、`min_properties` 这类内部键往往没有补全提示。

本仓库当前的策略是：

- 保持单一 `tool_schema(...)` 语法，不拆成多套接口
- 在宏里给出更明确的编译错误
- 用本文件作为权威属性清单
