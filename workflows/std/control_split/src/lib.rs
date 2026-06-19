use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.control.split")
        .version("0.1.0")
        .name("Control Split")
        .description("Split a JSON array or object into first, rest, and items outputs.")
        .input("value", "json")
        .input_description("value", "Source JSON value to split.")
        .input_required("value", true)
        .input_widget("value", "json")
        .output("first", "json")
        .output_description("first", "First array item, first object entry, or the scalar value.")
        .output("rest", "json")
        .output_description("rest", "Remaining array items or object entries.")
        .output("items", "json")
        .output_description("items", "Array representation of the source value.")
        .runtime("control_split", "lightflow.control.split")
        .build()
}
