use fastnbt::Value;

pub fn to_snbt(value : &Value) -> String {
    match value {
        Value::Byte(b) => format!("{}b", b),
        Value::Short(s) => format!("{}s", s),
        Value::Int(i) => format!("{}", i),
        Value::Long(l) => format!("{}l", l),
        Value::Float(f) => format!("{}f", f),
        Value::Double(d) => format!("{}d", d),
        Value::String(s) => format!("\"{}\"", s),
        Value::ByteArray(byte_array) => {
            let mut result = String::new();
            result.push_str("[B;");
            
            for byte in byte_array.iter() {
                if result.len() > 3 {
                    result.push(',');
                }
                result.push_str(&format!("{}b", byte));
            }

            result.push(']');
            result
        },
        Value::IntArray(int_array) => {
            let mut result = String::new();
            result.push_str("[I;");
            
            for int in int_array.iter() {
                if result.len() > 3 {
                    result.push(',');
                }
                result.push_str(&int.to_string());
            }

            result.push(']');
            result
        }
        Value::LongArray(long_array) => {
            let mut result = String::new();
            result.push_str("[L;");
            
            for long in long_array.iter() {
                if result.len() > 3 {
                    result.push(',');
                }
                result.push_str(&format!("{}l", long));
            }

            result.push(']');
            result
        },
        Value::List(values) => {
            let mut result = String::new();
            result.push('[');
            for (i, value) in values.iter().enumerate() {
                if i > 0 {
                    result.push(',');
                }
                result.push_str(&to_snbt(value));
            }
            result.push(']');
            result
        },
        Value::Compound(hash_map) => {
            let mut result = String::new();
            result.push('{');
            for (i, (key, value)) in hash_map.iter().enumerate() {
                if i > 0 {
                    result.push(',');
                }

                if key.contains(' ') || key.contains(':') || key.contains('{') || key.contains('}') {
                    result.push('"');
                    result.push_str(key);
                    result.push('"');
                } else {
                    result.push_str(key);
                }

                result.push(':');
                result.push_str(&to_snbt(value));
            }
            result.push('}');
            result
        }
    }
}