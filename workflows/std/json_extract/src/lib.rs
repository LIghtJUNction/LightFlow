use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.json.extract")
        .version("0.1.0")
        .name("JSON Extract")
        .description("Extract a value from JSON by dot path.")
        .input("value", "json")
        .input_description("value", "Source JSON value.")
        .input_required("value", true)
        .input_widget("value", "json")
        .input("path", "text")
        .input_description("path", "Dot path to extract, such as user.name or items.0.title.")
        .input_required("path", true)
        .input_widget("path", "text")
        .output("value", "json")
        .output_description("value", "Extracted JSON value, or null when missing.")
        .output("text", "text")
        .output_description("text", "Extracted value coerced to text.")
        .output("found", "boolean")
        .output_description("found", "Whether a non-null value was found.")
        .runtime("json_extract", "lightflow.json.extract")
        .build()
}
