use tree_sitter::{Parser, Tree, Node};

pub fn parse_source(_filename: &str, src: &str) -> Option<Tree> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_python::language()).ok()?;
    parser.parse(src, None)
}

pub fn root<'a>(tree: &'a Tree) -> Node<'a> {
    tree.root_node()
}

pub fn extract_functions_and_methods(src: &str) -> (Vec<String>, Vec<(String, String)>) {
    let tree = match parse_source("impl.py", src) {
        Some(t) => t,
        None => return (vec![], vec![]),
    };

    let root = tree.root_node();
    let mut functions = Vec::new();
    let mut methods = Vec::new();

    let mut w = root.walk();
    for node in root.children(&mut w) {
        if node.kind() == "function_definition" {
            if let Some(name) = node.child_by_field_name("name") {
                if let Ok(n) = name.utf8_text(src.as_bytes()) {
                    functions.push(n.to_string());
                }
            }
        }

        if node.kind() == "class_definition" {
            let class_name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(src.as_bytes()).ok())
                .unwrap_or("")
                .to_string();

            if let Some(body) = node.child_by_field_name("body") {
                let mut w2 = body.walk();
                for child in body.children(&mut w2) {
                    if child.kind() == "function_definition" {
                        if let Some(n) = child.child_by_field_name("name") {
                            if let Ok(m) = n.utf8_text(src.as_bytes()) {
                                methods.push((class_name.clone(), m.to_string()));
                            }
                        }
                    }
                }
            }
        }
    }

    (functions, methods)
}
