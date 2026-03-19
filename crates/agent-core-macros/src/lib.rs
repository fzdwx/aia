use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    parse_macro_input, Attribute, Data, DataStruct, DeriveInput, Expr, Fields, GenericArgument,
    LitInt, LitStr, PathArguments, Type,
};

#[proc_macro_derive(ToolArgsSchema, attributes(serde, tool_schema))]
pub fn derive_tool_args_schema(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_tool_args_schema(&input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand_tool_args_schema(input: &DeriveInput) -> syn::Result<TokenStream2> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "ToolArgsSchema derive 首轮不支持泛型参数",
        ));
    }

    let ident = &input.ident;
    let min_properties = parse_container_min_properties(&input.attrs)?;

    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            ident,
            "ToolArgsSchema derive 首轮仅支持命名字段 struct",
        ));
    };

    let fields = collect_struct_fields(data)?;
    let mut schema = quote!(::agent_core::ToolSchema::object());
    if let Some(minimum) = min_properties {
        schema = quote!(#schema.min_properties(#minimum));
    }

    for field in fields {
        let name = field.schema_name;
        let property_expr = field.property_expr;
        let required = field.required;
        schema = quote!(#schema.property(#name, #property_expr, #required));
    }

    Ok(quote! {
        impl ::agent_core::ToolArgsSchema for #ident {
            fn schema() -> ::agent_core::ToolSchema {
                #schema
            }
        }
    })
}

struct SchemaField {
    schema_name: String,
    property_expr: TokenStream2,
    required: bool,
}

struct FieldSchemaConfig {
    description: Option<String>,
    minimum: Option<i64>,
    maximum: Option<i64>,
    metadata: Vec<FieldSchemaMeta>,
}

struct FieldSchemaMeta {
    key: String,
    value: FieldSchemaMetaValue,
}

enum FieldSchemaMetaValue {
    String(String),
    Boolean(bool),
    Integer(i64),
}

fn collect_struct_fields(data: &DataStruct) -> syn::Result<Vec<SchemaField>> {
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            &data.fields,
            "ToolArgsSchema derive 首轮仅支持命名字段 struct",
        ));
    };

    fields
        .named
        .iter()
        .map(|field| {
            let Some(ident) = &field.ident else {
                return Err(syn::Error::new_spanned(field, "字段必须具名"));
            };

            let schema_name =
                parse_serde_rename(&field.attrs)?.unwrap_or_else(|| ident.to_string());
            let config = parse_field_schema_config(&field.attrs)?;
            let (property_expr, required) = property_expr(&field.ty, &config)?;

            Ok(SchemaField { schema_name, property_expr, required })
        })
        .collect()
}

fn property_expr(ty: &Type, config: &FieldSchemaConfig) -> syn::Result<(TokenStream2, bool)> {
    let (inner_ty, required) = unwrap_option(ty).map_or((ty, true), |inner| (inner, false));
    let primitive_kind = primitive_kind(inner_ty)?;
    let mut expr = match primitive_kind {
        PrimitiveKind::String => quote!(::agent_core::ToolSchemaProperty::string()),
        PrimitiveKind::Boolean => quote!(::agent_core::ToolSchemaProperty::boolean()),
        PrimitiveKind::StringArray => {
            quote!(::agent_core::ToolSchemaProperty::array(
                ::agent_core::ToolSchemaProperty::string()
            ))
        }
        PrimitiveKind::UnsignedInteger => {
            quote!(::agent_core::ToolSchemaProperty::integer().minimum(0u64))
        }
        PrimitiveKind::SignedInteger => quote!(::agent_core::ToolSchemaProperty::integer()),
    };

    if let Some(description) = config.description.as_deref() {
        expr = quote!(#expr.description(#description));
    }

    if let Some(minimum) = config.minimum {
        if matches!(
            primitive_kind,
            PrimitiveKind::String | PrimitiveKind::Boolean | PrimitiveKind::StringArray
        ) {
            return Err(syn::Error::new_spanned(
                ty,
                "tool_schema(minimum = ...) 仅支持整数类型字段",
            ));
        }
        if matches!(primitive_kind, PrimitiveKind::UnsignedInteger) && minimum < 0 {
            return Err(syn::Error::new_spanned(ty, "无符号整数字段的 minimum 不能为负数"));
        }
        expr = quote!(#expr.minimum(#minimum));
    }

    if let Some(maximum) = config.maximum {
        if matches!(
            primitive_kind,
            PrimitiveKind::String | PrimitiveKind::Boolean | PrimitiveKind::StringArray
        ) {
            return Err(syn::Error::new_spanned(
                ty,
                "tool_schema(maximum = ...) 仅支持整数类型字段",
            ));
        }
        if matches!(primitive_kind, PrimitiveKind::UnsignedInteger) && maximum < 0 {
            return Err(syn::Error::new_spanned(ty, "无符号整数字段的 maximum 不能为负数"));
        }
        expr = quote!(#expr.maximum(#maximum));
    }

    for meta in &config.metadata {
        let key = &meta.key;
        let meta_expr = match &meta.value {
            FieldSchemaMetaValue::String(value) => {
                quote!(::agent_core::ToolSchemaMetadataValue::String(#value.into()))
            }
            FieldSchemaMetaValue::Boolean(value) => {
                quote!(::agent_core::ToolSchemaMetadataValue::Boolean(#value))
            }
            FieldSchemaMetaValue::Integer(value) => {
                quote!(::agent_core::ToolSchemaMetadataValue::Integer(#value))
            }
        };
        expr = quote!(#expr.meta(#key, #meta_expr));
    }

    Ok((expr, required))
}

enum PrimitiveKind {
    String,
    Boolean,
    StringArray,
    UnsignedInteger,
    SignedInteger,
}

fn primitive_kind(ty: &Type) -> syn::Result<PrimitiveKind> {
    let Type::Path(type_path) = ty else {
        return Err(syn::Error::new_spanned(
            ty,
            "ToolArgsSchema derive 当前支持 String、bool、Vec<String>、usize/u32/u64、isize/i32/i64 与它们的 Option 形式",
        ));
    };

    let Some(segment) = type_path.path.segments.last() else {
        return Err(syn::Error::new_spanned(ty, "无法识别字段类型"));
    };

    match segment.ident.to_string().as_str() {
        "String" => Ok(PrimitiveKind::String),
        "bool" => Ok(PrimitiveKind::Boolean),
        "Vec" => match &segment.arguments {
            PathArguments::AngleBracketed(arguments) => match arguments.args.first() {
                Some(GenericArgument::Type(Type::Path(inner_path)))
                    if inner_path.path.is_ident("String") =>
                {
                    Ok(PrimitiveKind::StringArray)
                }
                _ => Err(syn::Error::new_spanned(
                    ty,
                    "ToolArgsSchema derive 首轮仅支持 Vec<String>，暂不支持其他数组元素类型",
                )),
            },
            _ => Err(syn::Error::new_spanned(
                ty,
                "ToolArgsSchema derive 首轮仅支持 Vec<String>，暂不支持其他数组元素类型",
            )),
        },
        "usize" | "u32" | "u64" => Ok(PrimitiveKind::UnsignedInteger),
        "isize" | "i32" | "i64" => Ok(PrimitiveKind::SignedInteger),
        _ => Err(syn::Error::new_spanned(
            ty,
            "ToolArgsSchema derive 当前支持 String、bool、Vec<String>、usize/u32/u64、isize/i32/i64 与它们的 Option 形式",
        )),
    }
}

fn unwrap_option(ty: &Type) -> Option<&Type> {
    let Type::Path(type_path) = ty else {
        return None;
    };
    let segment = type_path.path.segments.last()?;
    if segment.ident != "Option" {
        return None;
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    let Some(GenericArgument::Type(inner_ty)) = arguments.args.first() else {
        return None;
    };
    Some(inner_ty)
}

fn parse_container_min_properties(attrs: &[Attribute]) -> syn::Result<Option<u64>> {
    let mut result = None;
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("tool_schema")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("min_properties") {
                let value = meta.value()?;
                let literal: LitInt = value.parse()?;
                result = Some(literal.base10_parse()?);
                return Ok(());
            }
            Err(meta.error(
                "容器级仅支持 #[tool_schema(min_properties = N)]；当前支持键：min_properties",
            ))
        })?;
    }

    Ok(result)
}

fn parse_field_schema_config(attrs: &[Attribute]) -> syn::Result<FieldSchemaConfig> {
    let mut result =
        FieldSchemaConfig { description: None, minimum: None, maximum: None, metadata: Vec::new() };
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("tool_schema")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("description") {
                let value = meta.value()?;
                let literal: LitStr = value.parse()?;
                result.description = Some(literal.value());
                return Ok(());
            }
            if meta.path.is_ident("minimum") {
                let value = meta.value()?;
                result.minimum = Some(parse_signed_integer_expr(&value.parse()?)?);
                return Ok(());
            }
            if meta.path.is_ident("maximum") {
                let value = meta.value()?;
                result.maximum = Some(parse_signed_integer_expr(&value.parse()?)?);
                return Ok(());
            }
            if meta.path.is_ident("meta") {
                result.metadata.push(parse_field_schema_meta(&meta)?);
                return Ok(());
            }
            Err(meta.error(
                "字段级仅支持 #[tool_schema(description = \"...\", minimum = N, maximum = N, meta(key = \"...\", value = ...))]；当前支持键：description、minimum、maximum、meta",
            ))
        })?;
    }

    Ok(result)
}

fn parse_field_schema_meta(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<FieldSchemaMeta> {
    let mut key = None;
    let mut value = None;

    meta.parse_nested_meta(|nested| {
        if nested.path.is_ident("key") {
            let literal: LitStr = nested.value()?.parse()?;
            key = Some(literal.value());
            return Ok(());
        }
        if nested.path.is_ident("value") {
            let expr: Expr = nested.value()?.parse()?;
            value = Some(parse_field_schema_meta_value(&expr)?);
            return Ok(());
        }
        Err(nested.error("meta(...) 仅支持 key = \"...\" 与 value = ... 两个键"))
    })?;

    let Some(key) = key else {
        return Err(meta.error("meta(...) 缺少 key = \"...\""));
    };
    let Some(value) = value else {
        return Err(meta.error("meta(...) 缺少 value = ..."));
    };

    if matches!(key.as_str(), "description" | "minimum" | "maximum") {
        return Err(meta.error("meta(key = ...) 不能覆盖 description、minimum、maximum 等内建键"));
    }

    Ok(FieldSchemaMeta { key, value })
}

fn parse_field_schema_meta_value(expr: &Expr) -> syn::Result<FieldSchemaMetaValue> {
    match expr {
        Expr::Lit(expr_lit) => match &expr_lit.lit {
            syn::Lit::Str(literal) => Ok(FieldSchemaMetaValue::String(literal.value())),
            syn::Lit::Bool(literal) => Ok(FieldSchemaMetaValue::Boolean(literal.value())),
            syn::Lit::Int(literal) => Ok(FieldSchemaMetaValue::Integer(literal.base10_parse()?)),
            _ => Err(syn::Error::new_spanned(
                expr,
                "meta(value = ...) 当前只支持字符串、布尔与整数字面量",
            )),
        },
        Expr::Unary(expr_unary) if expr_unary.op == syn::UnOp::Neg(Default::default()) => {
            let Expr::Lit(expr_lit) = &*expr_unary.expr else {
                return Err(syn::Error::new_spanned(
                    expr,
                    "meta(value = ...) 当前只支持字符串、布尔与整数字面量",
                ));
            };
            let syn::Lit::Int(literal) = &expr_lit.lit else {
                return Err(syn::Error::new_spanned(
                    expr,
                    "meta(value = ...) 当前只支持字符串、布尔与整数字面量",
                ));
            };
            Ok(FieldSchemaMetaValue::Integer(-literal.base10_parse::<i64>()?))
        }
        _ => Err(syn::Error::new_spanned(
            expr,
            "meta(value = ...) 当前只支持字符串、布尔与整数字面量",
        )),
    }
}

fn parse_signed_integer_expr(expr: &Expr) -> syn::Result<i64> {
    match expr {
        Expr::Lit(expr_lit) => {
            let syn::Lit::Int(literal) = &expr_lit.lit else {
                return Err(syn::Error::new_spanned(expr, "数值约束只支持整数字面量"));
            };
            literal.base10_parse()
        }
        Expr::Unary(expr_unary) if expr_unary.op == syn::UnOp::Neg(Default::default()) => {
            let Expr::Lit(expr_lit) = &*expr_unary.expr else {
                return Err(syn::Error::new_spanned(expr, "数值约束只支持整数字面量"));
            };
            let syn::Lit::Int(literal) = &expr_lit.lit else {
                return Err(syn::Error::new_spanned(expr, "数值约束只支持整数字面量"));
            };
            Ok(-literal.base10_parse::<i64>()?)
        }
        _ => Err(syn::Error::new_spanned(expr, "数值约束只支持整数字面量")),
    }
}

fn parse_serde_rename(attrs: &[Attribute]) -> syn::Result<Option<String>> {
    let mut rename = None;
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("serde")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                let value = meta.value()?;
                let literal: LitStr = value.parse()?;
                rename = Some(literal.value());
                return Ok(());
            }
            if meta.path.is_ident("flatten") {
                return Err(meta.error("ToolArgsSchema derive 首轮不支持 serde(flatten)"));
            }
            if meta.path.is_ident("rename_all") {
                return Err(meta.error("ToolArgsSchema derive 首轮不支持 serde(rename_all)"));
            }
            if meta.path.is_ident("deny_unknown_fields") {
                return Ok(());
            }
            if meta.path.is_ident("default") || meta.path.is_ident("untagged") {
                return Ok(());
            }
            if let Ok(_value) = meta.value() {
                return Ok(());
            }
            Ok(())
        })?;
    }
    Ok(rename)
}
