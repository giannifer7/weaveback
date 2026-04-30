// weaveback-docgen/src/xref/analysis.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

pub(in crate::xref) fn collect_use_tree(tree: &syn::UseTree, prefix: &str, out: &mut Vec<String>) {
    match tree {
        syn::UseTree::Path(p) => {
            let new_prefix = format!("{}{}::", prefix, p.ident);
            collect_use_tree(&p.tree, &new_prefix, out);
        }
        syn::UseTree::Name(n) => {
            out.push(format!("{}{}", prefix, n.ident));
        }
        syn::UseTree::Rename(r) => {
            out.push(format!("{}{}", prefix, r.ident));
        }
        syn::UseTree::Glob(_) => {
            // glob — record prefix so we know there's a dependency
            if !prefix.is_empty() {
                out.push(prefix.trim_end_matches("::").to_string());
            }
        }
        syn::UseTree::Group(g) => {
            for item in &g.items {
                collect_use_tree(item, prefix, out);
            }
        }
    }
}

pub(in crate::xref) fn is_pub(vis: &syn::Visibility) -> bool {
    matches!(vis, syn::Visibility::Public(_))
}

pub(in crate::xref) fn collect_items(items: &[syn::Item], use_paths: &mut Vec<String>, symbols: &mut Vec<String>) {
    for item in items {
        match item {
            syn::Item::Use(u) => {
                collect_use_tree(&u.tree, "", use_paths);
            }
            syn::Item::Fn(f) if is_pub(&f.vis) => {
                symbols.push(f.sig.ident.to_string());
            }
            syn::Item::Struct(s) if is_pub(&s.vis) => {
                symbols.push(s.ident.to_string());
            }
            syn::Item::Enum(e) if is_pub(&e.vis) => {
                symbols.push(e.ident.to_string());
            }
            syn::Item::Trait(t) if is_pub(&t.vis) => {
                symbols.push(t.ident.to_string());
            }
            syn::Item::Type(t) if is_pub(&t.vis) => {
                symbols.push(t.ident.to_string());
            }
            syn::Item::Const(c) if is_pub(&c.vis) => {
                symbols.push(c.ident.to_string());
            }
            syn::Item::Static(s) if is_pub(&s.vis) => {
                symbols.push(s.ident.to_string());
            }
            syn::Item::Mod(m) if is_pub(&m.vis) => {
                symbols.push(m.ident.to_string());
                if let Some((_, inner_items)) = &m.content {
                    collect_items(inner_items, use_paths, symbols);
                }
            }
            _ => {}
        }
    }
}

pub(in crate::xref) fn analyze_file(path: &Path) -> (Vec<String>, Vec<String>) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (vec![], vec![]),
    };
    let file = match syn::parse_file(&content) {
        Ok(f) => f,
        Err(_) => return (vec![], vec![]),
    };
    let mut use_paths = Vec::new();
    let mut symbols = Vec::new();
    collect_items(&file.items, &mut use_paths, &mut symbols);
    (use_paths, symbols)
}

