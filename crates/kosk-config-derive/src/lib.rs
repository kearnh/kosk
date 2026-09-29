//! Macros for single-definition config settings.
//!
//! `#[config_section]` reads `#[config(default = ...)]` and `#[setting(...)]`
//! on struct fields. It generates the `Default` impl and the settings
//! descriptors, and it writes the `#[serde(default)]` attributes so a field
//! is declared once. `Choice` generates the
//! [`crate::config::schema::Choice`] impl for unit-only enums.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{punctuated::Punctuated, Data, DeriveInput, Expr, Fields, Token};

const SCHEMA: &str = "crate::config::schema";

fn schema_path(ident: &str) -> TokenStream2 {
    let path: syn::Path = syn::parse_str(&format!("{SCHEMA}::{ident}")).unwrap();
    quote!(#path)
}

/// `AboveField` -> `Above field`.
fn split_words(name: &str) -> String {
    let mut out = String::new();
    for (i, ch) in name.char_indices() {
        if i > 0 && ch.is_uppercase() && name[..i].ends_with(|c: char| c.is_lowercase()) {
            out.push(' ');
        }
        out.push(ch);
    }
    let mut chars = out.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
    }
}

#[proc_macro_derive(Choice, attributes(choice))]
pub fn derive_choice(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    match choice_impl(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn choice_impl(input: DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let choice_trait = schema_path("Choice");
    let data_enum = match &input.data {
        Data::Enum(data) => data,
        _ => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "Choice can only be derived for enums",
            ));
        }
    };
    let mut all = Vec::new();
    let mut arms = Vec::new();
    for variant in &data_enum.variants {
        if !matches!(variant.fields, Fields::Unit) {
            return Err(syn::Error::new_spanned(
                &variant.ident,
                "Choice variants must be unit variants",
            ));
        }
        let ident = &variant.ident;
        all.push(quote!(#name::#ident));
        let mut label: Option<String> = None;
        for attr in &variant.attrs {
            if attr.path().is_ident("choice") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("label") {
                        let value: syn::LitStr = meta.value()?.parse()?;
                        label = Some(value.value());
                        Ok(())
                    } else {
                        Err(meta.error("unsupported choice attribute"))
                    }
                })?;
            }
        }
        let text = label.unwrap_or_else(|| split_words(&ident.to_string()));
        arms.push(quote!(#name::#ident => #text));
    }
    Ok(quote! {
        impl #choice_trait for #name {
            const ALL: &'static [Self] = &[#(#all),*];
            fn label(&self) -> &'static str {
                match self {
                    #(#arms),*
                }
            }
        }
    })
}

struct LeafSetting {
    label: String,
    explain: String,
    page: Option<TokenStream2>,
    range: Option<(TokenStream2, TokenStream2)>,
    step: Option<TokenStream2>,
    decimals: Option<u32>,
    unit: Option<String>,
    advanced: bool,
    mirror: Option<syn::Ident>,
}

fn parse_page(expr: &Expr) -> syn::Result<TokenStream2> {
    let page = schema_path("Page");
    let device_page = schema_path("DevicePage");
    match expr {
        Expr::Path(path) => {
            let ident = path
                .path
                .get_ident()
                .ok_or_else(|| syn::Error::new_spanned(expr, "page must be a name"))?;
            Ok(quote!(#page::#ident))
        }
        Expr::Call(call) => {
            let func = match call.func.as_ref() {
                Expr::Path(path) => path
                    .path
                    .get_ident()
                    .ok_or_else(|| syn::Error::new_spanned(expr, "page must be Device(..)"))?,
                _ => return Err(syn::Error::new_spanned(expr, "page must be Device(..)")),
            };
            if func != "Device" {
                return Err(syn::Error::new_spanned(
                    expr,
                    "page must be a name or Device(..)",
                ));
            }
            let mut args = call.args.iter();
            let target = args
                .next()
                .ok_or_else(|| syn::Error::new_spanned(expr, "Device needs a controller"))?;
            let sheet = args
                .next()
                .ok_or_else(|| syn::Error::new_spanned(expr, "Device needs a page"))?;
            if args.next().is_some() {
                return Err(syn::Error::new_spanned(expr, "Device takes two arguments"));
            }
            Ok(
                quote!(#page::Device(crate::controller::ControllerKind::#target, #device_page::#sheet)),
            )
        }
        _ => Err(syn::Error::new_spanned(
            expr,
            "page must be a name or Device(..)",
        )),
    }
}

enum SettingAttr {
    None,
    Section { page: Option<TokenStream2> },
    Leaf(Box<LeafSetting>),
}

fn parse_setting_attr(field: &syn::Field) -> syn::Result<SettingAttr> {
    let mut section = false;
    let mut leaf = LeafSetting {
        label: String::new(),
        explain: String::new(),
        page: None,
        range: None,
        step: None,
        decimals: None,
        unit: None,
        advanced: false,
        mirror: None,
    };
    let mut has_leaf_key = false;
    for attr in &field.attrs {
        if !attr.path().is_ident("setting") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("section") {
                section = true;
                Ok(())
            } else if meta.path.is_ident("label") {
                let value: syn::LitStr = meta.value()?.parse()?;
                leaf.label = value.value();
                has_leaf_key = true;
                Ok(())
            } else if meta.path.is_ident("explain") {
                let value: syn::LitStr = meta.value()?.parse()?;
                leaf.explain = value.value();
                has_leaf_key = true;
                Ok(())
            } else if meta.path.is_ident("page") {
                let value: Expr = meta.value()?.parse()?;
                leaf.page = Some(parse_page(&value)?);
                Ok(())
            } else if meta.path.is_ident("range") {
                let value: Expr = meta.value()?.parse()?;
                match &value {
                    Expr::Range(range) => {
                        let start = range.start.as_ref().ok_or_else(|| {
                            syn::Error::new_spanned(&value, "range needs a start")
                        })?;
                        let end = range
                            .end
                            .as_ref()
                            .ok_or_else(|| syn::Error::new_spanned(&value, "range needs an end"))?;
                        leaf.range = Some((quote!(#start), quote!(#end)));
                        has_leaf_key = true;
                        Ok(())
                    }
                    _ => Err(syn::Error::new_spanned(
                        &value,
                        "range must look like 0..=500",
                    )),
                }
            } else if meta.path.is_ident("step") {
                let value: Expr = meta.value()?.parse()?;
                leaf.step = Some(quote!(#value));
                has_leaf_key = true;
                Ok(())
            } else if meta.path.is_ident("decimals") {
                let value: syn::LitInt = meta.value()?.parse()?;
                leaf.decimals = Some(value.base10_parse()?);
                has_leaf_key = true;
                Ok(())
            } else if meta.path.is_ident("unit") {
                let value: syn::LitStr = meta.value()?.parse()?;
                leaf.unit = Some(value.value());
                has_leaf_key = true;
                Ok(())
            } else if meta.path.is_ident("advanced") {
                leaf.advanced = true;
                Ok(())
            } else if meta.path.is_ident("mirror") {
                let value: Expr = meta.value()?.parse()?;
                match &value {
                    Expr::Path(path) => {
                        leaf.mirror = Some(path.path.get_ident().cloned().ok_or_else(|| {
                            syn::Error::new_spanned(&value, "mirror needs a field")
                        })?);
                        has_leaf_key = true;
                        Ok(())
                    }
                    _ => Err(syn::Error::new_spanned(&value, "mirror needs a field")),
                }
            } else {
                Err(meta.error("unsupported setting attribute"))
            }
        })?;
    }
    if section {
        // Page is allowed on sections; leaf-only keys are not.
        if has_leaf_key || leaf.advanced {
            return Err(syn::Error::new_spanned(
                field,
                "section fields take no label/explain/range/step/decimals/unit/mirror/advanced",
            ));
        }
        if leaf.mirror.is_some() {
            return Err(syn::Error::new_spanned(
                field,
                "mirror is only valid on a value field",
            ));
        }
        return Ok(SettingAttr::Section { page: leaf.page });
    }
    if leaf.label.is_empty()
        && leaf.explain.is_empty()
        && leaf.range.is_none()
        && leaf.step.is_none()
        && leaf.decimals.is_none()
        && leaf.unit.is_none()
        && leaf.mirror.is_none()
        && !leaf.advanced
        && leaf.page.is_none()
    {
        return Ok(SettingAttr::None);
    }
    if leaf.label.is_empty() || leaf.explain.is_empty() {
        return Err(syn::Error::new_spanned(
            field,
            "settings need both label and explain",
        ));
    }
    Ok(SettingAttr::Leaf(Box::new(leaf)))
}

fn field_is_flatten(field: &syn::Field) -> bool {
    field.attrs.iter().any(|attr| {
        if !attr.path().is_ident("serde") {
            return false;
        }
        let Ok(list) = attr.meta.require_list() else {
            return false;
        };
        list.parse_args_with(Punctuated::<syn::Meta, Token![,]>::parse_terminated)
            .map(|metas| {
                metas.iter().any(|meta| match meta {
                    syn::Meta::Path(path) => path.is_ident("flatten"),
                    _ => false,
                })
            })
            .unwrap_or(false)
    })
}

fn config_default(field: &syn::Field) -> syn::Result<Option<TokenStream2>> {
    for attr in &field.attrs {
        if !attr.path().is_ident("config") {
            continue;
        }
        let mut default: Option<TokenStream2> = None;
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("default") {
                let value: Expr = meta.value()?.parse()?;
                default = Some(quote!(#value));
                Ok(())
            } else {
                Err(meta.error("unsupported config attribute"))
            }
        })?;
        return Ok(default);
    }
    Ok(None)
}

fn type_name(ty: &syn::Type) -> String {
    quote!(#ty).to_string().replace(' ', "")
}

#[allow(clippy::too_many_arguments)]
fn leaf_collector(
    parent: &syn::Ident,
    field: &syn::Field,
    leaf: LeafSetting,
    setting_ty: &TokenStream2,
    page_ty: &TokenStream2,
    bool_control: &TokenStream2,
    num_control: &TokenStream2,
    choice_control: &TokenStream2,
    mirror_control: &TokenStream2,
    leak_key: &TokenStream2,
) -> syn::Result<TokenStream2> {
    let ident = field.ident.as_ref().unwrap();
    let ty = &field.ty;
    if leaf.mirror.is_some() && leaf.range.is_some() {
        return Err(syn::Error::new_spanned(
            field,
            "mirror is only valid on choice fields",
        ));
    }
    let page_tokens = leaf
        .page
        .unwrap_or(quote!(inherited.expect("setting needs a page")));
    let field_name = ident.to_string();
    let label = &leaf.label;
    let explain = &leaf.explain;
    let advanced = leaf.advanced;
    let ty_name = type_name(ty);
    let control = if ty_name == "bool" {
        if leaf.range.is_some()
            || leaf.step.is_some()
            || leaf.decimals.is_some()
            || leaf.mirror.is_some()
        {
            return Err(syn::Error::new_spanned(
                field,
                "booleans take no range/step/decimals/mirror",
            ));
        }
        quote! {
            #bool_control { lens: __lens }
        }
    } else if leaf.range.is_some() {
        let (min, max) = leaf.range.unwrap();
        let step = leaf
            .step
            .ok_or_else(|| syn::Error::new_spanned(field, "ranged settings need step"))?;
        if leaf.mirror.is_some() {
            return Err(syn::Error::new_spanned(
                field,
                "mirror is only valid on choice fields",
            ));
        }
        if ty_name == "f32" {
            let decimals = leaf
                .decimals
                .ok_or_else(|| syn::Error::new_spanned(field, "f32 settings need decimals"))?;
            let unit = match leaf.unit {
                Some(unit) => quote!(Some(#unit)),
                None => quote!(None),
            };
            quote! {
                #num_control { lens: __lens, min: #min, max: #max, step: #step, digits: #decimals, unit: #unit }
            }
        } else {
            if leaf.decimals.is_some() {
                return Err(syn::Error::new_spanned(
                    field,
                    "decimals is only valid on f32 settings",
                ));
            }
            let unit = match leaf.unit {
                Some(unit) => quote!(Some(#unit)),
                None => quote!(None),
            };
            quote! {
                #num_control { lens: __lens, min: #min, max: #max, step: #step, digits: 0, unit: #unit }
            }
        }
    } else {
        if leaf.step.is_some() || leaf.decimals.is_some() || leaf.unit.is_some() {
            return Err(syn::Error::new_spanned(
                field,
                "choice settings take no range/step/decimals/unit",
            ));
        }
        match leaf.mirror {
            Some(mirror) => quote! {
                #mirror_control {
                    lens: __lens,
                    mirror: base.field(|p: &#parent| &p.#mirror, |p: &mut #parent| &mut p.#mirror),
                }
            },
            None => quote! {
                #choice_control { lens: __lens }
            },
        }
    };
    Ok(quote! {
        {
            let __lens = base.field(|p: &#parent| &p.#ident, |p: &mut #parent| &mut p.#ident);
            let __page: #page_ty = #page_tokens;
            out.push(#setting_ty {
                key: #leak_key(format!("{prefix}{}", #field_name)),
                page: __page,
                label: #label,
                explain: #explain,
                advanced: #advanced,
                control: Box::new(#control),
            });
        }
    })
}

#[proc_macro_attribute]
pub fn config_section(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut input = syn::parse_macro_input!(item as DeriveInput);
    match config_section_impl(&mut input) {
        Ok(impls) => quote!(#input #impls).into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn serde_specifies_default(attr: &syn::Attribute) -> bool {
    if !attr.path().is_ident("serde") {
        return false;
    }
    let mut found = false;
    let Ok(()) = attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("default") {
            found = true;
        }
        if meta.input.peek(Token![=]) {
            let _value: Expr = meta.value()?.parse()?;
        }
        Ok(())
    }) else {
        return false;
    };
    found
}

fn is_bare_serde_default(attr: &syn::Attribute) -> bool {
    if !attr.path().is_ident("serde") {
        return false;
    }
    let mut metas = Vec::new();
    let Ok(()) = attr.parse_nested_meta(|meta| {
        let bare = meta.path.is_ident("default") && !meta.input.peek(Token![=]);
        if meta.input.peek(Token![=]) {
            let _value: Expr = meta.value()?.parse()?;
        }
        metas.push(bare);
        Ok(())
    }) else {
        return false;
    };
    metas == [true]
}

fn config_section_impl(input: &mut DeriveInput) -> syn::Result<TokenStream2> {
    if !input.attrs.iter().any(serde_specifies_default) {
        input.attrs.push(syn::parse_quote!(#[serde(default)]));
    }

    let name = input.ident.clone();
    let fields = match &mut input.data {
        Data::Struct(data) => match &mut data.fields {
            Fields::Named(fields) => fields,
            _ => {
                return Err(syn::Error::new_spanned(
                    name,
                    "config_section needs named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                name,
                "config_section can only be applied to structs",
            ));
        }
    };

    let setting_ty = schema_path("Setting");
    let lens_ty = schema_path("Lens");
    let page_ty = schema_path("Page");
    let bool_control = schema_path("BoolControl");
    let num_control = schema_path("NumControl");
    let choice_control = schema_path("ChoiceControl");
    let mirror_control = schema_path("MirrorChoiceControl");
    let leak_key = schema_path("leak_key");

    let mut default_inits = Vec::new();
    let mut sdef_fns = Vec::new();
    let mut collectors = Vec::new();

    for field in &mut fields.named {
        let ident = field.ident.clone().unwrap();
        let ty = field.ty.clone();
        let default = config_default(field)?;
        let attr = parse_setting_attr(field)?;
        let default_expr: TokenStream2 = match &default {
            Some(expr) => expr.clone(),
            None => quote!(Default::default()),
        };
        default_inits.push(quote!(#ident: #default_expr));
        let serde_owns_default = field.attrs.iter().any(serde_specifies_default);
        if default.is_some() && !serde_owns_default {
            let sdef = format_ident!("__sdef_{}", ident);
            let fn_path = format!("{name}::{sdef}");
            field
                .attrs
                .push(syn::parse_quote!(#[serde(default = #fn_path)]));
            sdef_fns.push(quote! {
                fn #sdef() -> #ty {
                    #default_expr
                }
            });
        }
        if default.is_none() {
            field.attrs.retain(|attr| !is_bare_serde_default(attr));
        }
        field
            .attrs
            .retain(|attr| !attr.path().is_ident("config") && !attr.path().is_ident("setting"));

        let SettingAttr::Section { page } = attr else {
            let leaf = match attr {
                SettingAttr::Leaf(leaf) => *leaf,
                _ => continue,
            };
            collectors.push(leaf_collector(
                &name,
                field,
                leaf,
                &setting_ty,
                &page_ty,
                &bool_control,
                &num_control,
                &choice_control,
                &mirror_control,
                &leak_key,
            )?);
            continue;
        };
        {
            let page_tokens = page
                .map(|page| quote!(Some(#page)))
                .unwrap_or(quote!(inherited));
            let prefix_tokens = if field_is_flatten(field) {
                quote!(prefix.clone())
            } else {
                let field_name = ident.to_string();
                quote!(format!("{prefix}{}.", #field_name))
            };
            collectors.push(quote! {
                {
                    let __page = #page_tokens;
                    let __prefix: String = #prefix_tokens;
                    <#ty>::__kosk_collect(
                        &base.field(|p: &#name| &p.#ident, |p: &mut #name| &mut p.#ident),
                        __page,
                        __prefix,
                        out,
                    );
                }
            });
            continue;
        }
    }

    Ok(quote! {
        impl ::std::default::Default for #name {
            fn default() -> Self {
                Self {
                    #(#default_inits),*
                }
            }
        }

        impl #name {
            #(#sdef_fns)*

            // Generated code appends to `out`, so it needs the `Vec`, not a slice.
            #[allow(clippy::ptr_arg)]
            pub(crate) fn __kosk_collect(
                base: &#lens_ty<Self>,
                inherited: Option<#page_ty>,
                prefix: String,
                out: &mut Vec<#setting_ty>,
            ) {
                #(#collectors)*
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    fn expand(body: &str) -> syn::Result<TokenStream2> {
        let mut input: DeriveInput = syn::parse_str(&format!("struct Tiny {{ {body} }}"))?;
        let impls = config_section_impl(&mut input)?;
        Ok(quote!(#input #impls))
    }

    #[test]
    fn leaf_and_section_attributes_parse() {
        let tokens = expand(
            r#"
            #[config(default = 1.0)]
            #[setting(page = Overlay, label = "L", explain = "E", range = 0.0..=1.0, step = 0.05, decimals = 2)]
            opacity: f32,
            #[setting(section, page = Device(Sc2, Pads))]
            sub: Sub,
            plain: u64
            "#,
        )
        .unwrap()
        .to_string();
        assert!(tokens.contains("__sdef_opacity"));
        assert!(tokens.contains("__kosk_collect"));
        assert!(tokens.contains("serde"));
        assert!(!tokens.contains("setting"));
    }

    #[test]
    fn explicit_serde_default_is_kept() {
        let tokens = expand(
            r#"
            #[config(default = 2)]
            #[serde(default)]
            version: i64,
            "#,
        )
        .unwrap()
        .to_string();
        assert!(!tokens.contains("__sdef_version"));
        assert!(tokens.contains("serde"));
    }

    #[test]
    fn section_rejects_leaf_keys() {
        let err =
            expand(r#"#[setting(section, label = "L", explain = "E")] sub: Sub"#).unwrap_err();
        assert!(err.to_string().contains("section fields take no"));
    }

    #[test]
    fn leaf_needs_label_and_explain() {
        let err = expand(r#"#[setting(page = Overlay)] flag: bool"#).unwrap_err();
        assert!(err.to_string().contains("both label and explain"));
    }
}
