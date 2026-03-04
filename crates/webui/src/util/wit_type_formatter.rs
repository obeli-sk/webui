use std::fmt::Write;
use val_json::type_wrapper::TypeWrapper;

/// Format a WIT type string with proper indentation and newlines.
/// Returns the original string if parsing fails.
pub fn format_wit_type(wit_type_inline: &str) -> String {
    match wit_type_inline.parse::<TypeWrapper>() {
        Ok(ty) => pretty_print(&ty, 0),
        Err(_) => wit_type_inline.to_string(),
    }
}

fn pretty_print(ty: &TypeWrapper, indent: usize) -> String {
    let indent_str = "  ".repeat(indent);
    let next_indent = "  ".repeat(indent + 1);

    match ty {
        TypeWrapper::Bool => "bool".to_string(),
        TypeWrapper::S8 => "s8".to_string(),
        TypeWrapper::U8 => "u8".to_string(),
        TypeWrapper::S16 => "s16".to_string(),
        TypeWrapper::U16 => "u16".to_string(),
        TypeWrapper::S32 => "s32".to_string(),
        TypeWrapper::U32 => "u32".to_string(),
        TypeWrapper::S64 => "s64".to_string(),
        TypeWrapper::U64 => "u64".to_string(),
        TypeWrapper::F32 => "f32".to_string(),
        TypeWrapper::F64 => "f64".to_string(),
        TypeWrapper::Char => "char".to_string(),
        TypeWrapper::String => "string".to_string(),
        TypeWrapper::Own => "own".to_string(),
        TypeWrapper::Borrow => "borrow".to_string(),

        TypeWrapper::List(inner) => {
            let inner_str = pretty_print(inner, indent);
            format!("list<{inner_str}>")
        }

        TypeWrapper::Option(inner) => {
            let inner_str = pretty_print(inner, indent);
            format!("option<{inner_str}>")
        }

        TypeWrapper::Tuple(items) => {
            if items.is_empty() {
                return "tuple<>".to_string();
            }
            let items_str: Vec<_> = items.iter().map(|t| pretty_print(t, indent)).collect();
            format!("tuple<{}>", items_str.join(", "))
        }

        TypeWrapper::Result { ok, err } => match (ok, err) {
            (None, None) => "result".to_string(),
            (Some(ok), None) => format!("result<{}>", pretty_print(ok, indent)),
            (None, Some(err)) => format!("result<_, {}>", pretty_print(err, indent)),
            (Some(ok), Some(err)) => {
                format!(
                    "result<{}, {}>",
                    pretty_print(ok, indent),
                    pretty_print(err, indent)
                )
            }
        },

        TypeWrapper::Record(fields) => {
            if fields.is_empty() {
                return "record {}".to_string();
            }
            let mut out = String::from("record {\n");
            for (i, (key, ty)) in fields.iter().enumerate() {
                let ty_str = pretty_print(ty, indent + 1);
                let _ = write!(out, "{}{}: {}", next_indent, key.as_kebab_str(), ty_str);
                if i < fields.len() - 1 {
                    out.push(',');
                }
                out.push('\n');
            }
            let _ = write!(out, "{}}}", indent_str);
            out
        }

        TypeWrapper::Variant(cases) => {
            if cases.is_empty() {
                return "variant {}".to_string();
            }
            let mut out = String::from("variant {\n");
            for (i, (key, payload)) in cases.iter().enumerate() {
                let _ = write!(out, "{}{}", next_indent, key.as_kebab_str());
                if let Some(ty) = payload {
                    let ty_str = pretty_print(ty, indent + 1);
                    let _ = write!(out, "({})", ty_str);
                }
                if i < cases.len() - 1 {
                    out.push(',');
                }
                out.push('\n');
            }
            let _ = write!(out, "{}}}", indent_str);
            out
        }

        TypeWrapper::Enum(cases) => {
            if cases.is_empty() {
                return "enum {}".to_string();
            }
            let mut out = String::from("enum {\n");
            for (i, key) in cases.iter().enumerate() {
                let _ = write!(out, "{}{}", next_indent, key.as_kebab_str());
                if i < cases.len() - 1 {
                    out.push(',');
                }
                out.push('\n');
            }
            let _ = write!(out, "{}}}", indent_str);
            out
        }

        TypeWrapper::Flags(flags) => {
            if flags.is_empty() {
                return "flags {}".to_string();
            }
            let mut out = String::from("flags {\n");
            for (i, key) in flags.iter().enumerate() {
                let _ = write!(out, "{}{}", next_indent, key.as_kebab_str());
                if i < flags.len() - 1 {
                    out.push(',');
                }
                out.push('\n');
            }
            let _ = write!(out, "{}}}", indent_str);
            out
        }
    }
}
