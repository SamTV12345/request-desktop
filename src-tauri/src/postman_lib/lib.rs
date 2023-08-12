use std::{fs::File, io::Read, path::Path};

use serde::{Deserialize, Serialize};
use crate::postman_lib::{v1_0_0, v2_0_0, v2_1_0};


pub const MINIMUM_POSTMAN_COLLECTION_VERSION: &str = ">= 1.0.0";

/// Errors that Postman Collection functions may return
/// Errors that Postman Collection functions may return
pub mod errors {
    use error_chain::error_chain;
    use crate::postman_lib::lib::MINIMUM_POSTMAN_COLLECTION_VERSION;
    error_chain! {
        foreign_links {
            Io(::std::io::Error);
            Yaml(::serde_yaml::Error);
            Serialize(::serde_json::Error);
            SemVerError(::semver::Error);
        }

        errors {
            UnsupportedSpecFileVersion(version: ::semver::Version) {
                description("Unsupported Postman Collection file version")
                display("Unsupported Postman Collection file version ({}). Expected {}", version, MINIMUM_POSTMAN_COLLECTION_VERSION)
            }
        }
    }
}

/// Supported versions of Postman Collection.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum PostmanCollection {
    /// Version 1.0.0 of the Postman Collection specification.
    ///
    /// Refer to the official
    /// [specification](https://schema.getpostman.com/collection/json/v1.0.0/draft-07/docs/index.html)
    /// for more information.
    #[allow(non_camel_case_types)]
    V1_0_0(v1_0_0::Spec),
    /// Version 1.0.0 of the Postman Collection specification.
    ///
    /// Refer to the official
    /// [specification](https://schema.getpostman.com/collection/json/v2.0.0/draft-07/docs/index.html)
    /// for more information.
    #[allow(non_camel_case_types)]
    V2_0_0(v2_0_0::Spec),
    /// Version 1.0.0 of the Postman Collection specification.
    ///
    /// Refer to the official
    /// [specification](https://schema.getpostman.com/collection/json/v2.1.0/draft-07/docs/index.html)
    /// for more information.
    #[allow(non_camel_case_types)]
    V2_1_0(v2_1_0::Spec),
}

/// Deserialize a Postman Collection from a path
pub fn from_path<P>(path: P) -> errors::Result<PostmanCollection>
    where
        P: AsRef<Path>,
{
    from_reader(File::open(path)?)
}

/// Deserialize a Postman Collection from type which implements Read
pub fn from_reader<R>(read: R) -> errors::Result<PostmanCollection>
    where
        R: Read,
{
    Ok(serde_yaml::from_reader::<R, PostmanCollection>(read)?)
}

/// Serialize Postman Collection spec to a YAML string
pub fn to_yaml(spec: &PostmanCollection) -> errors::Result<String> {
    Ok(serde_yaml::to_string(spec)?)
}

/// Serialize Postman Collection spec to JSON string
pub fn to_json(spec: &PostmanCollection) -> errors::Result<String> {
    Ok(serde_json::to_string_pretty(spec)?)
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;

    use glob::glob;

    use super::*;

    /// Helper function for reading a file to string.
    fn read_file<P>(path: P) -> String
        where
            P: AsRef<Path>,
    {
        let mut f = File::open(path).unwrap();
        let mut content = String::new();
        f.read_to_string(&mut content).unwrap();
        content
    }

    /// Helper function to write string to file.
    fn write_to_file<P>(path: P, filename: &str, data: &str)
        where
            P: AsRef<Path> + std::fmt::Debug,
    {
        println!("    Saving string to {:?}...", path);
        std::fs::create_dir_all(&path).unwrap();
        let full_filename = path.as_ref().to_path_buf().join(filename);
        let mut f = File::create(&full_filename).unwrap();
        f.write_all(data.as_bytes()).unwrap();
    }

    /// Convert a YAML `&str` to a JSON `String`.
    fn convert_yaml_str_to_json(yaml_str: &str) -> String {
        let yaml: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
        let json: serde_json::Value = serde_yaml::from_value(yaml).unwrap();
        serde_json::to_string_pretty(&json).unwrap()
    }

    /// Deserialize and re-serialize the input file to a JSON string through two different
    /// paths, comparing the result.
    /// 1. File -> `String` -> `serde_yaml::Value` -> `serde_json::Value` -> `String`
    /// 2. File -> `Spec` -> `serde_json::Value` -> `String`
    /// Both conversion of `serde_json::Value` -> `String` are done
    /// using `serde_json::to_string_pretty`.
    /// Since the first conversion is independent of the current crate (and only
    /// uses serde's json and yaml support), no information should be lost in the final
    /// JSON string. The second conversion goes through our `PostmanCollection`, so the final JSON
    /// string is a representation of _our_ implementation.
    /// By comparing those two JSON conversions, we can validate our implementation.
    fn compare_spec_through_json(
        input_file: &Path,
        save_path_base: &Path,
    ) -> (String, String, String) {
        // First conversion:
        //     File -> `String` -> `serde_yaml::Value` -> `serde_json::Value` -> `String`

        // Read the original file to string
        let spec_yaml_str = read_file(&input_file);
        // Convert YAML string to JSON string
        let spec_json_str = convert_yaml_str_to_json(&spec_yaml_str);

        // Second conversion:
        //     File -> `Spec` -> `serde_json::Value` -> `String`

        // Parse the input file
        let parsed_spec = from_path(&input_file).unwrap();
        // Convert to serde_json::Value
        let parsed_spec_json: serde_json::Value = serde_json::to_value(parsed_spec).unwrap();
        // Convert to a JSON string
        let parsed_spec_json_str: String = serde_json::to_string_pretty(&parsed_spec_json).unwrap();

        // Save JSON strings to file
        let api_filename = input_file
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .replace(".yaml", ".json");

        let mut save_path = save_path_base.to_path_buf();
        save_path.push("yaml_to_json");
        write_to_file(&save_path, &api_filename, &spec_json_str);

        let mut save_path = save_path_base.to_path_buf();
        save_path.push("yaml_to_spec_to_json");
        write_to_file(&save_path, &api_filename, &parsed_spec_json_str);

        // Return the JSON filename and the two JSON strings
        (api_filename, parsed_spec_json_str, spec_json_str)
    }

    // Just tests if the deserialization does not blow up. But does not test correctness
    #[test]
    fn can_deserialize() {
        for entry in glob("/tests/fixtures/collection/*.json").expect("Failed to read glob pattern")
        {
            let entry = entry.unwrap();
            let path = entry.as_path();
            // cargo test -- --nocapture to see this message
            println!("Testing if {:?} is deserializable", path);
            from_path(path).unwrap();
        }
    }

    #[test]
    fn can_deserialize_and_reserialize() {
        let save_path_base: std::path::PathBuf =
            ["target", "tests", "can_deserialize_and_reserialize"]
                .iter()
                .collect();
        let mut invalid_diffs = Vec::new();

        for entry in glob("/tests/fixtures/collection/*.json").expect("Failed to read glob pattern")
        {
            let entry = entry.unwrap();
            let path = entry.as_path();

            println!("Testing if {:?} is deserializable", path);

            let (api_filename, parsed_spec_json_str, spec_json_str) =
                compare_spec_through_json(path, &save_path_base);

            if parsed_spec_json_str != spec_json_str {
                invalid_diffs.push((api_filename, parsed_spec_json_str, spec_json_str));
            }
        }

        for invalid_diff in &invalid_diffs {
            println!("File {} failed JSON comparison!", invalid_diff.0);
        }
        assert_eq!(invalid_diffs.len(), 0);
    }
}
