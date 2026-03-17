use crate::value::Value;
use std::collections::HashMap;

pub fn render(template: &str, context: &HashMap<String, Value>) -> String {
    let mut result = template.to_string();
    for (key, value) in context {
        let placeholder = format!("{{{{ {} }}}}", key);
        result = result.replace(&placeholder, &value.to_string());
        // Also support without spaces
        let placeholder_no_space = format!("{{{{{}}}}}", key);
        result = result.replace(&placeholder_no_space, &value.to_string());
    }
    result
}
