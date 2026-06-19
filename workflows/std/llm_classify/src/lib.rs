use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.llm.classify")
        .version("0.1.0")
        .name("LLM Classify")
        .description("Classify text against a list of labels with deterministic offline matching.")
        .input("text", "text")
        .input_description("text", "Text to classify.")
        .input_required("text", true)
        .input_widget("text", "textarea")
        .input("labels", "json")
        .input_description("labels", "Array of candidate label strings.")
        .input_required("labels", true)
        .input_widget("labels", "json")
        .output("label", "text")
        .output_description("label", "Selected label.")
        .output("confidence", "number")
        .output_description("confidence", "Deterministic confidence score.")
        .runtime("llm_classify", "lightflow.llm.classify")
        .build()
}
