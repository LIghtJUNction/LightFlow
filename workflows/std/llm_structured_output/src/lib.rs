use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.llm.structured_output")
        .version("0.1.0")
        .name("LLM Structured Output")
        .description("Parse LLM text into JSON, or wrap plain text as a JSON object.")
        .input("text", "text")
        .input_description("text", "LLM text or JSON string.")
        .input_required("text", true)
        .input_widget("text", "textarea")
        .input("schema", "json")
        .input_description("schema", "Optional expected schema metadata for editor tooling.")
        .input_required("schema", false)
        .input_widget("schema", "json")
        .output("object", "json")
        .output_description("object", "Parsed JSON value or object containing the original text.")
        .output("json", "text")
        .output_description("json", "Serialized JSON output.")
        .runtime("llm_structured_output", "lightflow.llm.structured_output")
        .build()
}
