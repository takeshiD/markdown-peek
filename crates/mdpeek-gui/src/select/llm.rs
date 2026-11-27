//! General LLMs as selectors: the same questions as the System One request,
//! sent as a JSON prompt; the model answers with the same `answers` shape,
//! which [`super::jev::parse_response`] reads (only enumerated option ids
//! survive, so nothing the model writes becomes content).
//!
//! Protocols: Anthropic Messages, OpenAI Responses, OpenAI-compatible Chat
//! Completions (GLM, DeepSeek, OpenCode…), and the `claude` / `codex` CLIs.
//! The prompt goes to the CLIs on stdin, never in argv.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::jev::{auth_header, build_request, parse_response};
use super::models::{Api, Auth, ModelSpec};
use super::{SelectError, SelectionDecisions, SelectionRequest, Selector};

const SYSTEM: &str = "You choose reading aids for a Markdown viewer. Each section of the \
document lists candidate widgets that were pre-validated against its content. Prefer plain \
text unless a widget clearly helps a reader. Never assume facts beyond the given excerpts \
and features.\n\n\
Reply with one JSON object and nothing else: {\"answers\": {<question id>: <answer>}}. \
Answer every question.\n\
- A question of type \"noul\" is a yes/no question: its answer is the probability (a number \
from 0 to 1) that the answer is yes.\n\
- A question of type \"choice\" picks one key of its \"criteria\": its answer is \
{\"choice\": <key>, \"confidence\": <0 to 1>, \"probabilities\": {<key>: <0 to 1>, ...}}.";

pub struct LlmSelector {
    api: Api,
    model: String,
    url: String,
    auth: Auth,
    key: Option<String>,
    effort: Option<String>,
    sent: AtomicU32,
}

impl LlmSelector {
    pub fn new(spec: &ModelSpec, key: Option<String>) -> Result<Self, SelectError> {
        if spec.api.is_http() && !cfg!(feature = "llm") {
            return Err(SelectError::NotBuilt);
        }
        Ok(LlmSelector {
            api: spec.api,
            model: spec.model.clone(),
            url: spec.url(),
            auth: spec.auth,
            key,
            effort: spec.effort.clone(),
            sent: AtomicU32::new(0),
        })
    }
}

/// The user prompt: the System One request without the model id.
pub fn user_prompt(req: &SelectionRequest<'_>) -> String {
    let body = build_request(req, "");
    let payload = json!({ "state": body["state"], "questions": body["questions"] });
    format!(
        "The document state and the questions, as JSON:\n{}",
        serde_json::to_string(&payload).unwrap_or_default()
    )
}

/// Pull the first JSON object out of a model reply (code fences, stray prose).
pub fn extract_object(text: &str) -> Option<Value> {
    let t = text.trim();
    if let Ok(v) = serde_json::from_str::<Value>(t)
        && v.is_object()
    {
        return Some(v);
    }
    let start = t.find('{')?;
    let end = t.rfind('}')?;
    (end > start)
        .then(|| serde_json::from_str::<Value>(&t[start..=end]).ok())
        .flatten()
        .filter(Value::is_object)
}

/// The reply text of each HTTP protocol.
fn reply_text(api: Api, v: &Value) -> Option<String> {
    match api {
        Api::Anthropic => {
            let parts: Vec<&str> = v["content"]
                .as_array()?
                .iter()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect();
            (!parts.is_empty()).then(|| parts.join(""))
        }
        Api::Openai => {
            if let Some(s) = v["output_text"].as_str() {
                return Some(s.to_string());
            }
            let mut out = String::new();
            for item in v["output"].as_array()? {
                for c in item["content"].as_array().into_iter().flatten() {
                    if c["type"] == "output_text"
                        && let Some(s) = c["text"].as_str()
                    {
                        out.push_str(s);
                    }
                }
            }
            (!out.is_empty()).then_some(out)
        }
        Api::OpenaiChat => v["choices"][0]["message"]["content"]
            .as_str()
            .map(str::to_string),
        _ => None,
    }
}

/// Request body per protocol (pure; unit-tested).
pub fn http_body(api: Api, model: &str, effort: Option<&str>, user: &str) -> Value {
    match api {
        Api::Anthropic => {
            let mut b = json!({
                "model": model,
                "max_tokens": 8000,
                "system": SYSTEM,
                "messages": [{ "role": "user", "content": user }],
            });
            if let Some(e) = effort {
                b["output_config"] = json!({ "effort": e });
            }
            b
        }
        Api::Openai => {
            let mut b = json!({ "model": model, "instructions": SYSTEM, "input": user });
            if let Some(e) = effort {
                b["reasoning"] = json!({ "effort": e });
            }
            b
        }
        _ => {
            let mut b = json!({
                "model": model,
                "messages": [
                    { "role": "system", "content": SYSTEM },
                    { "role": "user", "content": user },
                ],
            });
            if let Some(e) = effort {
                b["reasoning_effort"] = json!(e);
            }
            b
        }
    }
}

impl LlmSelector {
    fn ask_http(
        &self,
        req: &SelectionRequest<'_>,
        deadline: Instant,
    ) -> Result<(u32, String), SelectError> {
        let key = self
            .key
            .as_deref()
            .ok_or(SelectError::MissingKey(String::new()))?;
        let body = http_body(
            self.api,
            &self.model,
            self.effort.as_deref(),
            &user_prompt(req),
        );
        let mut headers = vec![auth_header(self.auth, key)];
        if self.api == Api::Anthropic {
            headers.push(("anthropic-version", "2023-06-01".into()));
        }
        let (_, attempts, v) = super::http::post(super::http::Post {
            url: &self.url,
            headers,
            body: &body,
            deadline,
            cancel: req.cancel,
            sent: &self.sent,
        })?;
        let text = reply_text(self.api, &v).ok_or(SelectError::InvalidResponse)?;
        Ok((attempts, text))
    }

    fn ask_cli(
        &self,
        req: &SelectionRequest<'_>,
        deadline: Instant,
    ) -> Result<String, SelectError> {
        let prompt = format!("{SYSTEM}\n\n{}", user_prompt(req));
        let mut cmd = match self.api {
            Api::ClaudeCode => {
                let mut c = Command::new("claude");
                c.args(["-p", "--output-format", "text", "--max-turns", "1"]);
                if !self.model.is_empty() {
                    c.arg("--model").arg(&self.model);
                }
                c
            }
            _ => {
                let mut c = Command::new("codex");
                c.arg("exec");
                if !self.model.is_empty() {
                    c.arg("--model").arg(&self.model);
                }
                if let Some(e) = &self.effort {
                    c.arg("-c").arg(format!("model_reasoning_effort=\"{e}\""));
                }
                c.arg("-");
                c
            }
        };
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| SelectError::CliMissing)?;
        self.sent.fetch_add(1, Ordering::SeqCst);
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(prompt.as_bytes());
        }
        let mut stdout = child.stdout.take();
        let reader = std::thread::spawn(move || {
            let mut s = String::new();
            if let Some(o) = stdout.as_mut() {
                let _ = std::io::Read::read_to_string(o, &mut s);
            }
            s
        });
        loop {
            if req.cancel.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(SelectError::Cancelled);
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(SelectError::Timeout);
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    let out = reader.join().unwrap_or_default();
                    if !status.success() {
                        return Err(SelectError::CliFailed);
                    }
                    return Ok(out);
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(_) => return Err(SelectError::CliFailed),
            }
        }
    }
}

impl Selector for LlmSelector {
    fn last_requests(&self) -> Option<u32> {
        Some(self.sent.load(Ordering::SeqCst))
    }

    fn select(
        &self,
        req: &SelectionRequest<'_>,
        deadline: Instant,
    ) -> Result<SelectionDecisions, SelectError> {
        self.sent.store(0, Ordering::SeqCst);
        let started = Instant::now();
        let res = if self.api.is_http() {
            self.ask_http(req, deadline)
        } else {
            self.ask_cli(req, deadline).map(|t| (1, t))
        };
        let ms = started.elapsed().as_millis();
        let (attempts, text) = match res {
            Ok(x) => x,
            Err(e) => {
                tracing::warn!(target: "mdpeek::select", "{}: failed after {ms}ms ({})", self.api.as_str(), e.code());
                return Err(e);
            }
        };
        let parsed = extract_object(&text)
            .ok_or(SelectError::InvalidResponse)
            .and_then(|v| parse_response(&v, req));
        tracing::info!(
            target: "mdpeek::select",
            "{}: answered in {ms}ms, attempts {attempts}, sections {}{}",
            self.api.as_str(),
            req.sections.len(),
            if parsed.is_err() { ", invalid response" } else { "" }
        );
        parsed.map(|mut d| {
            d.requests = attempts;
            d
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_json_from_fenced_or_chatty_replies() {
        let v = extract_object("Sure!\n```json\n{\"answers\": {\"s1_helps\": 0.8}}\n```").unwrap();
        assert_eq!(v["answers"]["s1_helps"], 0.8);
        assert!(extract_object("no json here").is_none());
        assert!(extract_object("[1,2]").is_none());
    }

    #[test]
    fn bodies_per_protocol() {
        let a = http_body(Api::Anthropic, "claude-haiku-5-5", Some("low"), "U");
        assert_eq!(a["output_config"]["effort"], "low");
        assert_eq!(a["messages"][0]["content"], "U");
        let o = http_body(Api::Openai, "gpt-6-luna", None, "U");
        assert_eq!(o["input"], "U");
        assert!(o.get("reasoning").is_none());
        let c = http_body(Api::OpenaiChat, "glm-5.3", None, "U");
        assert_eq!(c["messages"][1]["content"], "U");
    }

    #[test]
    fn reply_text_per_protocol() {
        let a = json!({"content": [{"type": "thinking", "thinking": ""}, {"type": "text", "text": "{}"}]});
        assert_eq!(reply_text(Api::Anthropic, &a).as_deref(), Some("{}"));
        let o = json!({"output": [{"type": "message", "content": [{"type": "output_text", "text": "{\"a\":1}"}]}]});
        assert_eq!(reply_text(Api::Openai, &o).as_deref(), Some("{\"a\":1}"));
        let c = json!({"choices": [{"message": {"content": "{}"}}]});
        assert_eq!(reply_text(Api::OpenaiChat, &c).as_deref(), Some("{}"));
    }
}
