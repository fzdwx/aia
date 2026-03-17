use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    Attribute, Data, DataStruct, DeriveInput, Fields, GenericArgument, LitInt, LitStr,
    PathArguments, Type, parse_macro_input,
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
            let description = parse_field_description(&field.attrs)?;
            let (property_expr, required) = property_expr(&field.ty, description.as_deref())?;

            Ok(SchemaField { schema_name, property_expr, required })
        })
        .collect()
}

fn property_expr(ty: &Type, description: Option<&str>) -> syn::Result<(TokenStream2, bool)> {
    let (inner_ty, required) = unwrap_option(ty).map_or((ty, true), |inner| (inner, false));
    let mut expr = match primitive_kind(inner_ty)? {
        PrimitiveKind::String => quote!(::agent_core::ToolSchemaProperty::string()),
        PrimitiveKind::Integer => {
            quote!(::agent_core::ToolSchemaProperty::integer().minimum(0))
        }
    };

    if let Some(description) = description {
        expr = quote!(#expr.description(#description));
    }

    Ok((expr, required))
}

enum PrimitiveKind {
    String,
    Integer,
}

fn primitive_kind(ty: &Type) -> syn::Result<PrimitiveKind> {
    let Type::Path(type_path) = ty else {
        return Err(syn::Error::new_spanned(
            ty,
            "ToolArgsSchema derive 首轮仅支持 String、usize、u32、u64 与它们的 Option 形式",
        ));
    };

    let Some(segment) = type_path.path.segments.last() else {
        return Err(syn::Error::new_spanned(ty, "无法识别字段类型"));
    };

    match segment.ident.to_string().as_str() {
        "String" => Ok(PrimitiveKind::String),
        "usize" | "u32" | "u64" => Ok(PrimitiveKind::Integer),
        _ => Err(syn::Error::new_spanned(
            ty,
            "ToolArgsSchema derive 首轮仅支持 String、usize、u32、u64 与它们的 Option 形式",
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

fn parse_field_description(attrs: &[Attribute]) -> syn::Result<Option<String>> {
    let mut result = None;
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("tool_schema")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("description") {
                let value = meta.value()?;
                let literal: LitStr = value.parse()?;
                result = Some(literal.value());
                return Ok(());
            }
            Err(meta.error(
                "字段级仅支持 #[tool_schema(description = \"...\")]；当前支持键：description",
            ))
        })?;
    }

    Ok(result)
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
