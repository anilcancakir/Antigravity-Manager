use serde_json::Value;

/// 递归清理 JSON Schema 以符合 Gemini 接口要求
/// 
/// 1. [New] 展开 $ref 和 $defs: 将引用替换为实际定义，解决 Gemini 不支持 $ref 的问题
/// 2. 移除不支持的字段: $schema, additionalProperties, format, default, uniqueItems, validation fields
/// 3. 处理联合类型: ["string", "null"] -> "string"
/// 4. 将 type 字段的值转换为大写 (Gemini v1internal 要求)
/// 5. 移除数字校验字段: multipleOf, exclusiveMinimum, exclusiveMaximum 等
pub fn clean_json_schema(value: &mut Value) {
    // 0. 预处理：展开 $ref (Schema Flattening)
    if let Value::Object(map) = value {
        let mut defs = serde_json::Map::new();
        // 提取 $defs 或 definitions
        if let Some(Value::Object(d)) = map.remove("$defs") {
            defs.extend(d);
        }
        if let Some(Value::Object(d)) = map.remove("definitions") {
            defs.extend(d);
        }

        if !defs.is_empty() {
             // 递归替换引用
             flatten_refs(map, &defs);
        }
    }

    // 递归清理
    clean_json_schema_recursive(value);
}

/// 递归展开 $ref
fn flatten_refs(map: &mut serde_json::Map<String, Value>, defs: &serde_json::Map<String, Value>) {
    // 检查并替换 $ref
    if let Some(Value::String(ref_path)) = map.remove("$ref") {
        // 解析引用名 (例如 #/$defs/MyType -> MyType)
        let ref_name = ref_path.split('/').last().unwrap_or(&ref_path);
        
        if let Some(def_schema) = defs.get(ref_name) {
            // 将定义的内容合并到当前 map
            if let Value::Object(def_map) = def_schema {
                for (k, v) in def_map {
                    // 仅当当前 map 没有该 key 时才插入 (避免覆盖)
                    // 但通常 $ref 节点不应该有其他属性
                    map.entry(k.clone()).or_insert_with(|| v.clone());
                }
                
                // 递归处理刚刚合并进来的内容中可能包含的 $ref
                // 注意：这里可能会无限递归如果存在循环引用，但工具定义通常是 DAG
                flatten_refs(map, defs);
            }
        }
    }

    // 遍历子节点
    for (_, v) in map.iter_mut() {
        if let Value::Object(child_map) = v {
            flatten_refs(child_map, defs);
        } else if let Value::Array(arr) = v {
            for item in arr {
                if let Value::Object(item_map) = item {
                   flatten_refs(item_map, defs);
                }
            }
        }
    }
}

fn clean_json_schema_recursive(value: &mut Value) {
    match value {
        Value::Object(map) => {
            // 1. 收集并处理校验字段 (Soft-Remove: Move constraints to description)
            let validation_fields = [
                ("minLength", "minLen"),
                ("maxLength", "maxLen"),
                ("minimum", "min"),
                ("maximum", "max"),
                ("minItems", "minItems"),
                ("maxItems", "maxItems"),
                ("exclusiveMinimum", "exclMin"),
                ("exclusiveMaximum", "exclMax"),
                ("multipleOf", "multipleOf"),
                ("pattern", "pattern"),
            ];

            let mut constraints = Vec::new();
            for (field, label) in validation_fields {
                if let Some(val) = map.remove(field) {
                    constraints.push(format!("{}: {}", label, val));
                }
            }

            // 2. 将约束信息追加到描述
            if !constraints.is_empty() {
                let suffix = format!(" [Validation: {}]", constraints.join(", "));
                let desc = map.entry("description".to_string()).or_insert_with(|| Value::String("".to_string()));
                if let Value::String(s) = desc {
                    s.push_str(&suffix);
                }
            }

            // 3. 移除其他会干扰上游的非标准/冲突字段
            let other_fields_to_remove = [
                "$schema",
                "additionalProperties",
                "enumCaseInsensitive",
                "enumNormalizeWhitespace",
                "uniqueItems",
                "format",
                "default",
            ];
            for field in other_fields_to_remove {
                map.remove(field);
            }

            // 4. 处理 type 字段 (Gemini 要求小写，且支持联合类型)
            if let Some(type_val) = map.get_mut("type") {
                match type_val {
                    Value::String(s) => {
                        *type_val = Value::String(s.to_lowercase());
                    }
                    Value::Array(arr) => {
                        for item in arr {
                            if let Value::String(s) = item {
                                *item = Value::String(s.to_lowercase());
                            }
                        }
                    }
                    _ => {}
                }
            }

            // 3. 递归处理所有子节点 (Schema 中可能存在任意嵌套字段)
            for v in map.values_mut() {
                clean_json_schema_recursive(v);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                clean_json_schema_recursive(v);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_clean_json_schema_draft_2020_12() {
        let mut schema = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "location": {
                    "type": "string",
                    "minLength": 1,
                    "format": "city"
                },
                "unit": {
                    "type": ["string", "null"],
                    "default": "celsius"
                }
            },
            "required": ["location"]
        });

        clean_json_schema(&mut schema);

        // 1. 验证类型保持小写
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["location"]["type"], "string");

        // 2. 验证标准字段被转换并移动到描述 (Advanced Soft-Remove)
        assert!(schema["properties"]["location"].get("minLength").is_none());
        assert!(schema["properties"]["location"].get("format").is_none());
        assert!(schema["properties"]["location"]["description"].as_str().unwrap().contains("minLen: 1"));

        // 3. 验证联合类型保持数组形式且小写
        assert_eq!(schema["properties"]["unit"]["type"], json!(["string", "null"]));
        
        // 4. 验证元数据字段被移除
        assert!(schema.get("$schema").is_none());
    }

    #[test]
    fn test_flatten_refs() {
        let mut schema = json!({
            "$defs": {
                "Address": {
                    "type": "object",
                    "properties": {
                        "city": { "type": "string" }
                    }
                }
            },
            "properties": {
                "home": { "$ref": "#/$defs/Address" }
            }
        });

        clean_json_schema(&mut schema);

        // 验证引用被展开且类型转为小写
        assert_eq!(schema["properties"]["home"]["type"], "object");
        assert_eq!(schema["properties"]["home"]["properties"]["city"]["type"], "string");
    }
}
