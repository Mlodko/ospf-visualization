use super::gnmi;

pub fn parse_path(path: &str) -> gnmi::Path {
    gnmi::Path {
        elem: split_escaped(path.trim_start_matches('/'), '/')
            .into_iter()
            .filter(|element| !element.is_empty())
            .map(|element| parse_path_element(&element))
            .collect(),
        element: vec![],
        origin: String::new(),
        target: String::new(),
    }
}

pub fn extend_path(
    prefix: Option<&gnmi::Path>,
    suffix: Option<&gnmi::Path>,
) -> Option<gnmi::Path> {
    match (prefix, suffix) {
        (None, None) => None,
        (Some(path), None) | (None, Some(path)) => Some(path.clone()),
        (Some(prefix), Some(suffix)) => {
            let mut path = prefix.clone();
            path.elem.extend(suffix.elem.iter().cloned());

            if path.origin.is_empty() {
                path.origin.clone_from(&suffix.origin);
            }
            if path.target.is_empty() {
                path.target.clone_from(&suffix.target);
            }

            Some(path)
        }
    }
}

fn parse_path_element(element: &str) -> gnmi::PathElem {
    let (name, predicates) =
        split_escaped_once(element, '[').unwrap_or_else(|| (element.to_owned(), String::new()));
    let mut key = std::collections::HashMap::new();

    for predicate in split_escaped(&predicates, ']') {
        let predicate = predicate.trim_start_matches('[');
        if predicate.is_empty() {
            continue;
        }
        if let Some((key_name, value)) = split_escaped_once(predicate, '=') {
            key.insert(unescape(&key_name), unescape(&value));
        }
    }

    gnmi::PathElem {
        name: unescape(&name),
        key,
    }
}

fn split_escaped(input: &str, delimiter: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut escaped = false;

    for character in input.chars() {
        if escaped {
            current.push('\\');
            current.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == delimiter {
            parts.push(current);
            current = String::new();
        } else {
            current.push(character);
        }
    }

    if escaped {
        current.push('\\');
    }
    parts.push(current);
    parts
}

fn split_escaped_once(input: &str, delimiter: char) -> Option<(String, String)> {
    let mut escaped = false;
    for (index, character) in input.char_indices() {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == delimiter {
            return Some((
                input[..index].to_owned(),
                input[index + character.len_utf8()..].to_owned(),
            ));
        }
    }
    None
}

fn unescape(input: &str) -> String {
    let mut output = String::new();
    let mut escaped = false;
    for character in input.chars() {
        if escaped {
            output.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else {
            output.push(character);
        }
    }
    if escaped {
        output.push('\\');
    }
    output
}