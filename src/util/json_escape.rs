pub fn json_escape(src: &str) -> String {
    let mut escaped = String::with_capacity(src.len());
    for c in src.chars() {
        match c {
            '\x08' => escaped += "\\b",
            '\x0c' => escaped += "\\f",
            '\n' => escaped += "\\n",
            '\r' => escaped += "\\r",
            '\t' => escaped += "\\t",
            '"' => escaped += "\\\"",
            '\\' => escaped += "\\",
            c => escaped.push(c),
        }
    }
    escaped
}