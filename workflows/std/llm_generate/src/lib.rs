use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.llm.generate")
        .version("0.1.0")
        .name("LLM Generate")
        .description("Generate deterministic mock LLM text for offline workflow composition.")
        .input("prompt", "text")
        .input_description("prompt", "Prompt text.")
        .input_required("prompt", true)
        .input_widget("prompt", "prompt")
        .input("model", "text")
        .input_description("model", "Mock model name.")
        .input_required("model", false)
        .input_default_json("model", "\"mock\"")
        .input_widget("model", "text")
        .output("text", "text")
        .output_description("text", "Generated mock text.")
        .output("response", "text")
        .output_description("response", "Generated mock response.")
        .builtin_runtime("llm_mock", "lightflow.llm.generate", "builtin.llm.mock.v1")
        .build()
}
