use lightflow::asset::*;

pub const META: AssetMeta = AssetMeta {
    id: "workflow.text_plan",
    title: "Text Plan",
    kind: AssetKind::Workflow,
    description: "Drafts a concise plan from a text prompt through CortexFS.",
    stability: Stability::Draft,
};

pub fn define() -> WorkflowDef {
    workflow(META.id)
        .input_schema("schemas/text_plan.input.json")
        .output_schema("schemas/text_plan.output.json")
        .required_model("llm.planner")
        .api_step("draft", "node.llm_prompt", "openai.chat")
        .openai_chat_input("llm.planner", "prompt")
}
