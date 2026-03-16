use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input, Data, DeriveInput, Fields, FnArg, ImplItem, ItemImpl, LitStr, Pat, Token,
    Type,
};

// ─────────────────────────────────────────────
// #[controller(prefix = "/api/v1/admin/articles")]
// ─────────────────────────────────────────────

struct ControllerArgs {
    prefix: String,
    role: Option<String>,
    state: Option<String>,
}

impl Parse for ControllerArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut prefix = None;
        let mut role = None;
        let mut state = None;

        while !input.is_empty() {
            let ident: syn::Ident = input.parse()?;
            let _: Token![=] = input.parse()?;
            let lit: LitStr = input.parse()?;

            match ident.to_string().as_str() {
                "prefix" => prefix = Some(lit.value()),
                "role" => role = Some(lit.value()),
                "state" => state = Some(lit.value()),
                _ => return Err(syn::Error::new(ident.span(), "expected `prefix`, `role`, or `state`")),
            }

            // Consume optional comma
            let _ = input.parse::<Token![,]>();
        }

        Ok(ControllerArgs {
            prefix: prefix.unwrap_or_default(),
            role,
            state,
        })
    }
}

/// Route info extracted from method attributes
struct RouteInfo {
    method: String,
    path: String,
    fn_name: syn::Ident,
}

/// Find the parameter name that has type AuthGuard in a method signature
fn find_auth_guard_param(method: &syn::ImplItemFn) -> Option<syn::Ident> {
    for arg in &method.sig.inputs {
        if let FnArg::Typed(pat_type) = arg {
            if let Type::Path(type_path) = pat_type.ty.as_ref() {
                let last_seg = type_path.path.segments.last();
                if let Some(seg) = last_seg {
                    if seg.ident == "AuthGuard" {
                        // Extract the pattern name
                        if let Pat::Ident(pat_ident) = pat_type.pat.as_ref() {
                            return Some(pat_ident.ident.clone());
                        }
                    }
                }
            }
        }
    }
    None
}

/// Extracts route annotations from an impl method.
fn extract_route_info(item: &ImplItem) -> Option<RouteInfo> {
    let method_item = match item {
        ImplItem::Fn(m) => m,
        _ => return None,
    };

    let fn_name = method_item.sig.ident.clone();
    let mut http_method = None;
    let mut path = None;

    for attr in &method_item.attrs {
        let seg = attr.path().segments.last()?;
        let name = seg.ident.to_string();

        match name.as_str() {
            "get" | "post" | "put" | "delete" | "patch" => {
                http_method = Some(name);
                if let Ok(lit) = attr.parse_args::<LitStr>() {
                    path = Some(lit.value());
                }
            }
            _ => {}
        }
    }

    Some(RouteInfo {
        method: http_method?,
        path: path.unwrap_or_else(|| "/".to_string()),
        fn_name,
    })
}

/// # Controller macro
///
/// Generates a `router()` function from annotated methods.
/// `#[require_role("ROLE_X")]` auto-injects `auth.require_role("ROLE_X")?;`
/// at the beginning of the handler body.
///
/// ## Usage
/// ```ignore
/// #[controller(prefix = "/api/v1/admin/articles")]
/// impl ArticleController {
///     #[get("/")]
///     #[require_role("ROLE_ADMIN")]
///     async fn list(
///         auth: AuthGuard,
///         State(state): State<AppState>,
///     ) -> AppResult<impl IntoResponse> {
///         // auth.require_role("ROLE_ADMIN")?; ← auto-injected!
///         let items = ...;
///         Ok(Json(items))
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn controller(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as ControllerArgs);
    let mut impl_block = parse_macro_input!(input as ItemImpl);
    let prefix = &args.prefix;

    // Extract route info BEFORE modifying the impl block
    let routes: Vec<RouteInfo> = impl_block
        .items
        .iter()
        .filter_map(extract_route_info)
        .collect();

    let controller_role = args.role.clone();

    // Inject role checks and strip custom attributes
    for item in &mut impl_block.items {
        if let ImplItem::Fn(method) = item {
            // Find require_role value for this method (method-level overrides controller-level)
            let mut role_value = None;
            for attr in &method.attrs {
                let name = attr
                    .path()
                    .segments
                    .last()
                    .map(|s| s.ident.to_string())
                    .unwrap_or_default();
                if name == "require_role" {
                    if let Ok(lit) = attr.parse_args::<LitStr>() {
                        role_value = Some(lit.value());
                    }
                }
            }

            // Fall back to controller-level role if no method-level role
            let effective_role = role_value.or_else(|| controller_role.clone());

            // If a role is set, inject the check at the start of the body
            if let Some(role) = effective_role {
                if let Some(auth_param) = find_auth_guard_param(method) {
                    let check = syn::parse2::<syn::Stmt>(quote! {
                        #auth_param.require_role(#role)?;
                    })
                    .expect("Failed to parse role check statement");

                    method.block.stmts.insert(0, check);
                }
            }

            // Strip custom attributes
            method.attrs.retain(|attr| {
                let name = attr
                    .path()
                    .segments
                    .last()
                    .map(|s| s.ident.to_string())
                    .unwrap_or_default();
                !matches!(
                    name.as_str(),
                    "get" | "post" | "put" | "delete" | "patch" | "require_role"
                )
            });
        }
    }

    // Build route registrations grouped by path
    let mut path_groups: std::collections::BTreeMap<String, Vec<&RouteInfo>> =
        std::collections::BTreeMap::new();
    for route in &routes {
        path_groups
            .entry(route.path.clone())
            .or_default()
            .push(route);
    }

    let route_registrations: Vec<proc_macro2::TokenStream> = path_groups
        .iter()
        .map(|(path, methods)| {
            let mut chain = Vec::new();
            for (i, route) in methods.iter().enumerate() {
                let method_ident = format_ident!("{}", route.method);
                let fn_name = &route.fn_name;

                if i == 0 {
                    chain.push(quote! {
                        axum::routing::#method_ident(Self::#fn_name)
                    });
                } else {
                    chain.push(quote! {
                        .#method_ident(Self::#fn_name)
                    });
                }
            }

            quote! {
                .route(#path, #(#chain)*)
            }
        })
        .collect();

    let self_ty = &impl_block.self_ty;

    // Use custom state type if provided, otherwise default to karbon::http::AppState
    let state_type: proc_macro2::TokenStream = if let Some(ref state_path) = args.state {
        state_path.parse().unwrap_or_else(|_| quote! { karbon::http::AppState })
    } else {
        quote! { karbon::http::AppState }
    };

    let expanded = quote! {
        #impl_block

        impl #self_ty {
            /// Auto-generated router from #[controller] annotations
            pub fn router() -> axum::Router<#state_type> {
                axum::Router::new()
                    #(#route_registrations)*
            }

            /// Returns the route prefix for this controller
            pub fn prefix() -> &'static str {
                #prefix
            }
        }
    };

    TokenStream::from(expanded)
}

// ─────────────────────────────────────────────
// Standalone route macros (marker attributes, consumed by #[controller])
// ─────────────────────────────────────────────

/// Mark a handler as a GET route: `#[get("/path")]`
#[proc_macro_attribute]
pub fn get(_args: TokenStream, input: TokenStream) -> TokenStream {
    input
}

/// Mark a handler as a POST route: `#[post("/path")]`
#[proc_macro_attribute]
pub fn post(_args: TokenStream, input: TokenStream) -> TokenStream {
    input
}

/// Mark a handler as a PUT route: `#[put("/path")]`
#[proc_macro_attribute]
pub fn put(_args: TokenStream, input: TokenStream) -> TokenStream {
    input
}

/// Mark a handler as a DELETE route: `#[delete("/path")]`
#[proc_macro_attribute]
pub fn delete(_args: TokenStream, input: TokenStream) -> TokenStream {
    input
}

/// Mark a handler as a PATCH route: `#[patch("/path")]`
#[proc_macro_attribute]
pub fn patch(_args: TokenStream, input: TokenStream) -> TokenStream {
    input
}

/// Require a role — auto-injects `auth.require_role("ROLE")?;` at the start of the handler.
/// The handler must have an `AuthGuard` parameter.
/// Usage: `#[require_role("ROLE_ADMIN")]`
#[proc_macro_attribute]
pub fn require_role(_args: TokenStream, input: TokenStream) -> TokenStream {
    input
}

// ─────────────────────────────────────────────
// Helper: extract #[table_name = "..."] from attributes
// ─────────────────────────────────────────────

fn extract_table_name(attrs: &[syn::Attribute]) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident("table_name") {
            if let Ok(lit) = attr.parse_args::<LitStr>() {
                return Some(lit.value());
            }
        }
    }
    None
}

/// Check if a field has a specific helper attribute like #[auto_increment], #[skip_insert], etc.
fn has_field_attr(field: &syn::Field, name: &str) -> bool {
    field.attrs.iter().any(|attr| attr.path().is_ident(name))
}

/// Check if the struct has a specific attribute like #[timestamps]
fn has_struct_attr(attrs: &[syn::Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident(name))
}

/// Extract #[slug_from("field_name")] value from a field
fn extract_slug_from(field: &syn::Field) -> Option<String> {
    for attr in &field.attrs {
        if attr.path().is_ident("slug_from") {
            if let Ok(lit) = attr.parse_args::<LitStr>() {
                return Some(lit.value());
            }
        }
    }
    None
}

/// Check if a field type is Option<T>
fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(seg) = type_path.path.segments.last() {
            return seg.ident == "Option";
        }
    }
    false
}

// ─────────────────────────────────────────────
// #[derive(Insertable)]
// ─────────────────────────────────────────────

/// Derive macro that generates an `insert()` method for a struct.
///
/// ## Attributes
/// - `#[table_name("table")]` — required, specifies the DB table
/// - `#[auto_increment]` — on a field: exclude from INSERT (auto-generated by DB)
/// - `#[skip_insert]` — on a field: exclude from INSERT (e.g. created with DEFAULT)
/// - `#[timestamps]` — on the struct: auto-adds `created_at` column with `Utc::now()`
/// - `#[slug_from("field")]` — on a field: auto-generates slug from another field if empty
///
/// ## Example
/// ```ignore
/// #[derive(Insertable)]
/// #[table_name("posts")]
/// #[timestamps]
/// pub struct NewPost {
///     pub title: String,
///     #[slug_from("title")]
///     pub slug: String,
/// }
///
/// // Generated:
/// // dto.insert(pool).await?;
/// // → INSERT INTO posts (title, slug, created_at) VALUES (?, ?, ?)
/// // slug auto-generated from title if empty
/// ```
#[proc_macro_derive(Insertable, attributes(table_name, auto_increment, skip_insert, timestamps, slug_from))]
pub fn derive_insertable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let table = match extract_table_name(&input.attrs) {
        Some(t) => t,
        None => {
            return syn::Error::new_spanned(&input.ident, "Missing #[table_name(\"...\")]")
                .to_compile_error()
                .into();
        }
    };

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(f) => &f.named,
            _ => {
                return syn::Error::new_spanned(name, "Insertable only works on structs with named fields")
                    .to_compile_error()
                    .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(name, "Insertable only works on structs")
                .to_compile_error()
                .into();
        }
    };

    let has_timestamps = has_struct_attr(&input.attrs, "timestamps");

    // Collect fields that participate in INSERT (skip auto_increment and skip_insert)
    let insert_fields: Vec<_> = fields
        .iter()
        .filter(|f| !has_field_attr(f, "auto_increment") && !has_field_attr(f, "skip_insert"))
        .collect();

    let mut column_names: Vec<String> = insert_fields
        .iter()
        .map(|f| f.ident.as_ref().unwrap().to_string())
        .collect();

    // Auto-timestamps: add created_at column
    if has_timestamps {
        column_names.push("created_at".to_string());
    }


    // Build bind calls, handling #[slug_from] fields
    let mut bind_calls: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut slug_lets: Vec<proc_macro2::TokenStream> = Vec::new();

    for field in &insert_fields {
        let ident = field.ident.as_ref().unwrap();

        if let Some(source_field) = extract_slug_from(field) {
            let source_ident = format_ident!("{}", source_field);
            let var_name = format_ident!("__slug_{}", ident);
            slug_lets.push(quote! {
                let #var_name = if self.#ident.is_empty() {
                    slug::slugify(&self.#source_ident)
                } else {
                    self.#ident.clone()
                };
            });
            bind_calls.push(quote! { .bind(&#var_name) });
        } else {
            bind_calls.push(quote! { .bind(&self.#ident) });
        }
    }

    // Auto-timestamps: bind chrono::Utc::now()
    if has_timestamps {
        bind_calls.push(quote! { .bind(chrono::Utc::now()) });
    }

    // Build the SQL string at compile time
    let columns_str = column_names.join(", ");
    let placeholders_str: String = (1..=column_names.len())
        .map(|_| "?".to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let sql_literal = format!("INSERT INTO {} ({}) VALUES ({})", table, columns_str, placeholders_str);

    let expanded = quote! {
        impl #name {
            /// Auto-generated INSERT query from #[derive(Insertable)]
            pub async fn insert<'e, E>(&self, executor: E) -> karbon::error::AppResult<u64>
            where
                E: sqlx::Executor<'e, Database = karbon::db::Db>,
            {
                #(#slug_lets)*
                let result = sqlx::query(#sql_literal)
                    #(#bind_calls)*
                    .execute(executor)
                    .await?;
                Ok(karbon::db::last_insert_id(&result))
            }
        }
    };

    TokenStream::from(expanded)
}

// ─────────────────────────────────────────────
// #[derive(Updatable)]
// ─────────────────────────────────────────────

/// Derive macro that generates an `update()` method for a struct.
///
/// All `Option<T>` fields are treated as partial updates — only `Some` values
/// are included in the SET clause. Non-Option fields are always included.
///
/// ## Attributes
/// - `#[table_name("table")]` — required
/// - `#[primary_key]` — marks the WHERE clause field (required, exactly one)
/// - `#[skip_update]` — exclude from SET clause
///
/// ## Example
/// ```ignore
/// #[derive(Updatable)]
/// #[table_name("user")]
/// pub struct UpdateUser {
///     #[primary_key]
///     pub id: i64,
///     pub username: Option<String>,
///     pub email: Option<String>,
///     pub active: Option<bool>,
/// }
///
/// // Generated:
/// // UpdateUser { id: 42, username: Some("new"), email: None, active: None }.update(pool).await?;
/// // → UPDATE user SET username=? WHERE id=?  (only Some fields)
/// ```
#[proc_macro_derive(Updatable, attributes(table_name, primary_key, skip_update, timestamps))]
pub fn derive_updatable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let table = match extract_table_name(&input.attrs) {
        Some(t) => t,
        None => {
            return syn::Error::new_spanned(&input.ident, "Missing #[table_name(\"...\")]")
                .to_compile_error()
                .into();
        }
    };

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(f) => &f.named,
            _ => {
                return syn::Error::new_spanned(name, "Updatable only works on structs with named fields")
                    .to_compile_error()
                    .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(name, "Updatable only works on structs")
                .to_compile_error()
                .into();
        }
    };

    // Find the primary key field
    let pk_field = fields.iter().find(|f| has_field_attr(f, "primary_key"));
    let pk_field = match pk_field {
        Some(f) => f,
        None => {
            return syn::Error::new_spanned(name, "Updatable requires exactly one #[primary_key] field")
                .to_compile_error()
                .into();
        }
    };
    let pk_ident = pk_field.ident.as_ref().unwrap();
    let pk_col = pk_ident.to_string();

    let has_timestamps = has_struct_attr(&input.attrs, "timestamps");

    // Collect updatable fields (not primary_key, not skip_update)
    let update_fields: Vec<_> = fields
        .iter()
        .filter(|f| !has_field_attr(f, "primary_key") && !has_field_attr(f, "skip_update"))
        .collect();

    // Build the dynamic update logic with placeholder tracking
    let mut set_pushes = Vec::new();
    let mut bind_pushes = Vec::new();

    for field in &update_fields {
        let ident = field.ident.as_ref().unwrap();
        let col_name = ident.to_string();

        if is_option_type(&field.ty) {
            set_pushes.push(quote! {
                if self.#ident.is_some() {
                    param_idx += 1;
                    set_clauses.push(format!("{} = {}", #col_name, karbon::db::placeholder(param_idx)));
                }
            });
            bind_pushes.push(quote! {
                if let Some(ref val) = self.#ident {
                    query = query.bind(val);
                }
            });
        } else {
            set_pushes.push(quote! {
                param_idx += 1;
                set_clauses.push(format!("{} = {}", #col_name, karbon::db::placeholder(param_idx)));
            });
            bind_pushes.push(quote! {
                query = query.bind(&self.#ident);
            });
        }
    }

    // Auto-timestamps: add updated_at
    let timestamps_set_push = if has_timestamps {
        quote! {
            param_idx += 1;
            set_clauses.push(format!("{} = {}", "updated_at", karbon::db::placeholder(param_idx)));
        }
    } else {
        quote! {}
    };

    let timestamps_bind_push = if has_timestamps {
        quote! {
            query = query.bind(chrono::Utc::now());
        }
    } else {
        quote! {}
    };

    let expanded = quote! {
        impl #name {
            /// Auto-generated UPDATE query from #[derive(Updatable)]
            /// Only `Some` fields on Option<T> are included in SET.
            ///
            /// Accepts either a `&DbPool` or a `&mut Transaction`:
            /// ```ignore
            /// dto.update(pool).await?;           // normal
            /// dto.update(&mut *tx).await?;       // in transaction
            /// ```
            pub async fn update<'e, E>(&self, executor: E) -> karbon::error::AppResult<u64>
            where
                E: sqlx::Executor<'e, Database = karbon::db::Db>,
            {
                let mut set_clauses: Vec<String> = Vec::new();
                let mut param_idx: usize = 0;

                #(#set_pushes)*
                #timestamps_set_push

                if set_clauses.is_empty() {
                    return Ok(0); // nothing to update
                }

                param_idx += 1;
                let sql = format!(
                    "UPDATE {} SET {} WHERE {} = {}",
                    #table,
                    set_clauses.join(", "),
                    #pk_col,
                    karbon::db::placeholder(param_idx),
                );

                let mut query = sqlx::query(&sql);

                #(#bind_pushes)*
                #timestamps_bind_push

                // Bind the primary key for WHERE
                query = query.bind(&self.#pk_ident);

                let result = query.execute(executor).await?;
                Ok(result.rows_affected())
            }
        }
    };

    TokenStream::from(expanded)
}
