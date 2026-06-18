use super::{ApiError, ApiResult};
use crate::workflow::WorkflowSpec;

pub(super) fn execute_rig_llm(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<serde_json::Map<String, serde_json::Value>> {
    execute_rig_llm_impl(workflow, inputs)
}

#[cfg(not(feature = "rig"))]
fn execute_rig_llm_impl(
    workflow: &WorkflowSpec,
    _inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<serde_json::Map<String, serde_json::Value>> {
    Err(ApiError::InvalidRequest(format!(
        "workflow {} requires lightflow.llm.generate, but this LightFlow build was not compiled with --features rig",
        workflow.id
    )))
}

#[cfg(feature = "rig")]
fn execute_rig_llm_impl(
    _workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<serde_json::Map<String, serde_json::Value>> {
    let request = RigLlmRequest::from_inputs(inputs)?;
    let response = if request.provider == "mock" {
        format!("mock:{}:{}", request.model, request.prompt)
    } else {
        block_on_rig(run_rig_prompt(&request))??
    };

    let mut outputs = serde_json::Map::new();
    outputs.insert("text".to_owned(), response.clone().into());
    outputs.insert("response".to_owned(), response.into());
    outputs.insert("provider".to_owned(), request.provider.into());
    outputs.insert("model".to_owned(), request.model.into());
    Ok(outputs)
}

#[cfg(feature = "rig")]
#[derive(Debug, Clone)]
struct RigLlmRequest {
    provider: String,
    model: String,
    prompt: String,
    system: Option<String>,
    api_key: Option<String>,
    base_url: Option<String>,
    temperature: Option<f64>,
    max_tokens: Option<u64>,
    additional_params: Option<serde_json::Value>,
}

#[cfg(feature = "rig")]
impl RigLlmRequest {
    fn from_inputs(inputs: &serde_json::Map<String, serde_json::Value>) -> ApiResult<Self> {
        let provider = input_string(inputs, "provider")
            .or_else(|| std::env::var("LIGHTFLOW_RIG_PROVIDER").ok())
            .unwrap_or_else(|| "openai".to_owned())
            .to_ascii_lowercase();
        let model = input_string(inputs, "model")
            .or_else(|| std::env::var("LIGHTFLOW_RIG_MODEL").ok())
            .ok_or_else(|| {
                ApiError::InvalidRequest(
                    "model input or LIGHTFLOW_RIG_MODEL is required for RIG LLM generation"
                        .to_owned(),
                )
            })?;
        let prompt = input_string(inputs, "prompt")
            .or_else(|| input_string(inputs, "text"))
            .ok_or_else(|| {
                ApiError::InvalidRequest("prompt is required for RIG LLM generation".to_owned())
            })?;
        let system = input_string(inputs, "system")
            .or_else(|| input_string(inputs, "preamble"))
            .or_else(|| std::env::var("LIGHTFLOW_RIG_SYSTEM").ok());
        let api_key = input_string(inputs, "api_key").or_else(|| env_api_key(&provider));
        let base_url = input_string(inputs, "base_url").or_else(|| env_base_url(&provider));
        let temperature = input_f64(inputs, "temperature");
        let max_tokens = input_u64(inputs, "max_tokens");
        let additional_params = inputs.get("additional_params").cloned();

        Ok(Self {
            provider,
            model,
            prompt,
            system,
            api_key,
            base_url,
            temperature,
            max_tokens,
            additional_params,
        })
    }
}

#[cfg(feature = "rig")]
async fn run_rig_prompt(request: &RigLlmRequest) -> ApiResult<String> {
    match request.provider.as_str() {
        "openai" | "openai-compatible" | "openai_chat" | "openai-chat" => {
            let api_key = required_api_key(request, "OPENAI_API_KEY")?;
            let mut builder =
                rig_core::providers::openai::CompletionsClient::builder().api_key(api_key.as_str());
            if let Some(base_url) = &request.base_url {
                builder = builder.base_url(base_url);
            }
            let client = builder.build().map_err(rig_error)?;
            prompt_with_client(client, request).await
        }
        "openai-responses" | "openai_responses" => {
            let api_key = required_api_key(request, "OPENAI_API_KEY")?;
            let mut builder =
                rig_core::providers::openai::Client::builder().api_key(api_key.as_str());
            if let Some(base_url) = &request.base_url {
                builder = builder.base_url(base_url);
            }
            let client = builder.build().map_err(rig_error)?;
            prompt_with_client(client, request).await
        }
        "anthropic" | "claude" => {
            let api_key = required_api_key(request, "ANTHROPIC_API_KEY")?;
            let mut builder =
                rig_core::providers::anthropic::Client::builder().api_key(api_key.as_str());
            if let Some(base_url) = &request.base_url {
                builder = builder.base_url(base_url);
            }
            let client = builder.build().map_err(rig_error)?;
            prompt_with_client(client, request).await
        }
        "ollama" => {
            let api_key = request.api_key.clone().unwrap_or_default();
            let mut builder = rig_core::providers::ollama::Client::builder().api_key(api_key);
            if let Some(base_url) = &request.base_url {
                builder = builder.base_url(base_url);
            }
            let client = builder.build().map_err(rig_error)?;
            prompt_with_client(client, request).await
        }
        "openrouter" => {
            let api_key = required_api_key(request, "OPENROUTER_API_KEY")?;
            let mut builder =
                rig_core::providers::openrouter::Client::builder().api_key(api_key.as_str());
            if let Some(base_url) = &request.base_url {
                builder = builder.base_url(base_url);
            }
            let client = builder.build().map_err(rig_error)?;
            prompt_with_client(client, request).await
        }
        "deepseek" => {
            let api_key = required_api_key(request, "DEEPSEEK_API_KEY")?;
            let mut builder =
                rig_core::providers::deepseek::Client::builder().api_key(api_key.as_str());
            if let Some(base_url) = &request.base_url {
                builder = builder.base_url(base_url);
            }
            let client = builder.build().map_err(rig_error)?;
            prompt_with_client(client, request).await
        }
        "xai" | "x.ai" => {
            let api_key = required_api_key(request, "XAI_API_KEY")?;
            let mut builder = rig_core::providers::xai::Client::builder().api_key(api_key.as_str());
            if let Some(base_url) = &request.base_url {
                builder = builder.base_url(base_url);
            }
            let client = builder.build().map_err(rig_error)?;
            prompt_with_client(client, request).await
        }
        provider => Err(ApiError::InvalidRequest(format!(
            "unsupported RIG provider {provider}; supported providers: openai, openai-compatible, openai-responses, anthropic, ollama, openrouter, deepseek, xai, mock"
        ))),
    }
}

#[cfg(feature = "rig")]
async fn prompt_with_client<C>(client: C, request: &RigLlmRequest) -> ApiResult<String>
where
    C: rig_core::client::completion::CompletionClient,
    C::CompletionModel: 'static,
{
    use rig_core::completion::Prompt;

    let mut builder = client.agent(request.model.clone());
    if let Some(system) = &request.system {
        builder = builder.preamble(system);
    }
    if let Some(temperature) = request.temperature {
        builder = builder.temperature(temperature);
    }
    if let Some(max_tokens) = request.max_tokens {
        builder = builder.max_tokens(max_tokens);
    }
    if let Some(additional_params) = &request.additional_params {
        builder = builder.additional_params(additional_params.clone());
    }

    builder
        .build()
        .prompt(request.prompt.clone())
        .await
        .map_err(rig_error)
}

#[cfg(feature = "rig")]
fn block_on_rig<F>(future: F) -> ApiResult<F::Output>
where
    F: std::future::Future,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        Ok(tokio::task::block_in_place(|| handle.block_on(future)))
    } else {
        tokio::runtime::Runtime::new()
            .map_err(ApiError::from)
            .map(|runtime| runtime.block_on(future))
    }
}

#[cfg(feature = "rig")]
fn required_api_key(request: &RigLlmRequest, env_key: &str) -> ApiResult<String> {
    request.api_key.clone().ok_or_else(|| {
        ApiError::InvalidRequest(format!(
            "api_key input or {env_key} is required for provider {}",
            request.provider
        ))
    })
}

#[cfg(feature = "rig")]
fn env_api_key(provider: &str) -> Option<String> {
    let key = match provider {
        "anthropic" | "claude" => "ANTHROPIC_API_KEY",
        "ollama" => "OLLAMA_API_KEY",
        "openrouter" => "OPENROUTER_API_KEY",
        "deepseek" => "DEEPSEEK_API_KEY",
        "xai" | "x.ai" => "XAI_API_KEY",
        _ => "OPENAI_API_KEY",
    };
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

#[cfg(feature = "rig")]
fn env_base_url(provider: &str) -> Option<String> {
    let key = match provider {
        "anthropic" | "claude" => "ANTHROPIC_BASE_URL",
        "ollama" => "OLLAMA_API_BASE_URL",
        "openrouter" => "OPENROUTER_BASE_URL",
        "deepseek" => "DEEPSEEK_BASE_URL",
        "xai" | "x.ai" => "XAI_BASE_URL",
        _ => "OPENAI_BASE_URL",
    };
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

#[cfg(feature = "rig")]
fn input_string(inputs: &serde_json::Map<String, serde_json::Value>, name: &str) -> Option<String> {
    inputs.get(name).and_then(|value| match value {
        serde_json::Value::String(value) => Some(value.clone()),
        value if !value.is_null() => Some(value.to_string()),
        _ => None,
    })
}

#[cfg(feature = "rig")]
fn input_f64(inputs: &serde_json::Map<String, serde_json::Value>, name: &str) -> Option<f64> {
    inputs.get(name).and_then(serde_json::Value::as_f64)
}

#[cfg(feature = "rig")]
fn input_u64(inputs: &serde_json::Map<String, serde_json::Value>, name: &str) -> Option<u64> {
    inputs.get(name).and_then(serde_json::Value::as_u64)
}

#[cfg(feature = "rig")]
fn rig_error(error: impl std::fmt::Display) -> ApiError {
    ApiError::InvalidRequest(format!("RIG LLM generation failed: {error}"))
}
