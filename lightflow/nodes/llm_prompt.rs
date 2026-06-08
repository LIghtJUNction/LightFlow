use lightflow::asset::*;

pub const META: AssetMeta = AssetMeta {
    id: "node.llm_prompt",
    title: "LLM Prompt",
    kind: AssetKind::Node,
    description: "Formats prompt input for a CortexFS-backed language model step.",
    stability: Stability::Draft,
};
