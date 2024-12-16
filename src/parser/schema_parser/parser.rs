use crate::error_handling::Error::{
    IOError, InvalidSchema, MissingSchemaKey, NoneASCIICharacters, YamlParsingError,
};
use crate::error_handling::Result;
use crate::parser::regex_parser::parser::RegexParser;
use regex_syntax::ast::Ast;
use serde_yaml::Value;
use std::collections::{HashMap, HashSet};
use std::io::Read;

pub struct TimestampSchema {
    regex: String,
    ast: Ast,
}

impl TimestampSchema {
    pub fn new(regex: String) -> Result<TimestampSchema> {
        let mut regex_parser = RegexParser::new();
        let ast = regex_parser.parse_into_ast(regex.as_str())?;
        Ok(Self { regex, ast })
    }

    pub fn get_regex(&self) -> &str {
        &self.regex
    }

    pub fn get_ast(&self) -> &Ast {
        &self.ast
    }
}

pub struct VarSchema {
    pub name: String,
    pub regex: String,
    pub ast: Ast,
}

impl VarSchema {
    pub fn new(name: String, regex: String) -> Result<VarSchema> {
        let mut regex_parser = RegexParser::new();
        let ast = regex_parser.parse_into_ast(regex.as_str())?;
        Ok(Self { name, regex, ast })
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_regex(&self) -> &str {
        &self.regex
    }

    pub fn get_ast(&self) -> &Ast {
        &self.ast
    }
}

pub struct SchemaConfig {
    ts_schemas: Vec<TimestampSchema>,
    var_schemas: Vec<VarSchema>,
    delimiters: [bool; 128],
}

impl SchemaConfig {
    pub fn get_ts_schemas(&self) -> &Vec<TimestampSchema> {
        &self.ts_schemas
    }

    pub fn get_var_schemas(&self) -> &Vec<VarSchema> {
        &self.var_schemas
    }

    pub fn has_delimiter(&self, delimiter: char) -> bool {
        if false == delimiter.is_ascii() {
            return false;
        }
        self.delimiters[delimiter as usize]
    }
}

impl SchemaConfig {
    const TIMESTAMP_KEY: &'static str = "timestamp";
    const VAR_KEY: &'static str = "variables";
    const DELIMITER_EKY: &'static str = "delimiters";

    pub fn parse_from_str(yaml_content: &str) -> Result<SchemaConfig> {
        match Self::load_kv_pairs_from_yaml_content(yaml_content) {
            Ok(kv_pairs) => Self::load_from_kv_pairs(kv_pairs),
            Err(e) => Err(YamlParsingError(e)),
        }
    }

    pub fn parse_from_file(yaml_file_path: &str) -> Result<SchemaConfig> {
        match std::fs::File::open(yaml_file_path) {
            Ok(mut file) => {
                let mut contents = String::new();
                if let Err(e) = file.read_to_string(&mut contents) {
                    return Err(IOError(e));
                }
                Self::parse_from_str(contents.as_str())
            }
            Err(e) => Err(IOError(e)),
        }
    }

    fn get_key_value<'a>(
        kv_map: &'a HashMap<String, Value>,
        key: &'static str,
    ) -> Result<&'a Value> {
        kv_map.get(key).ok_or_else(|| MissingSchemaKey(key))
    }

    fn load_kv_pairs_from_yaml_content(
        yaml_content: &str,
    ) -> serde_yaml::Result<HashMap<String, Value>> {
        let kv_map_result: HashMap<String, Value> = serde_yaml::from_str(&yaml_content)?;
        Ok(kv_map_result)
    }

    fn load_from_kv_pairs(kv_pairs: HashMap<String, Value>) -> Result<Self> {
        // Handle timestamps
        let mut ts_schemas: Vec<TimestampSchema> = Vec::new();
        let timestamps = Self::get_key_value(&kv_pairs, Self::TIMESTAMP_KEY)?;
        if let Value::Sequence(sequence) = timestamps {
            sequence.iter().try_for_each(|val| {
                if let Value::String(s) = val {
                    ts_schemas.push(TimestampSchema::new(s.clone())?);
                    Ok(())
                } else {
                    Err(InvalidSchema)
                }
            })?;
        } else {
            return Err(InvalidSchema);
        }

        // Handle variables
        let mut var_schemas: Vec<VarSchema> = Vec::new();
        let vars = Self::get_key_value(&kv_pairs, Self::VAR_KEY)?;
        if let Value::Mapping(map) = vars {
            for (key, value) in map {
                match (key, value) {
                    (Value::String(name), Value::String(regex)) => {
                        var_schemas.push(VarSchema::new(name.clone(), regex.clone())?);
                    }
                    _ => return Err(InvalidSchema),
                }
            }
        } else {
            return Err(InvalidSchema);
        }

        // Handle delimiter
        let mut delimiters = [false; 128];
        let delimiter = Self::get_key_value(&kv_pairs, Self::DELIMITER_EKY)?;
        if let Value::String(delimiter_str) = delimiter {
            for c in delimiter_str.chars() {
                if false == c.is_ascii() {
                    return Err(NoneASCIICharacters);
                }
                delimiters[c as usize] = true;
            }
        } else {
            return Err(InvalidSchema);
        }
        delimiters['\n' as usize] = true;

        Ok((Self {
            ts_schemas,
            var_schemas,
            delimiters,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_example_schema_file() -> Result<()> {
        let project_root = env!("CARGO_MANIFEST_DIR");
        let schema_path = std::path::Path::new(project_root)
            .join("examples")
            .join("schema.yaml");
        let parsed_schema = SchemaConfig::parse_from_file(schema_path.to_str().unwrap())?;

        assert_eq!(parsed_schema.get_ts_schemas().len(), 3);
        assert_eq!(parsed_schema.get_var_schemas().len(), 4);

        let delimiters: Vec<char> = vec![' ', '\t', '\n', '\r', ':', ',', '!', ';', '%'];
        for delimiter in delimiters {
            assert!(parsed_schema.has_delimiter(delimiter));
        }

        Ok(())
    }
}
