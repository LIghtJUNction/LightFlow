use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.text.template")
        .version("0.1.0")
        .name("Text Template")
        .description("Render a text template with JSON variables.")
        .input("template", "text")
        .input_description("template", "Template text with placeholders such as {{topic}}.")
        .input_required("template", true)
        .input_widget("template", "textarea")
        .input("vars", "json")
        .input_description("vars", "JSON object used to fill template placeholders.")
        .input_required("vars", false)
        .input_default_json("vars", "{}")
        .input_widget("vars", "json")
        .output("text", "text")
        .output_description("text", "Rendered template text.")
        .runtime("text_template", "lightflow.text.template")
        .build()
}
