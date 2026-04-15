use serde::Deserialize;

#[derive(Debug)]
pub enum ParsedOutput {
    Text(String),
    JsonEvent(JsonOutputEvent),
    Empty,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JsonOutputEvent {
    #[serde(rename = "assistant")]
    Assistant {
        #[serde(default)]
        content: Vec<ContentBlock>,
    },
    #[serde(rename = "result")]
    Result {
        #[serde(default)]
        result: Option<String>,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        is_error: bool,
    },
    #[serde(rename = "system")]
    System {
        #[serde(default)]
        message: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text {
        #[serde(default)]
        text: String,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        #[serde(default)]
        name: String,
        #[serde(default)]
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        #[serde(default)]
        content: String,
    },
    #[serde(other)]
    Other,
}

pub struct OutputParser {
    is_json_mode: bool,
}

impl OutputParser {
    pub fn new(is_json_mode: bool) -> Self {
        Self { is_json_mode }
    }

    pub fn parse(&self, raw: &str) -> ParsedOutput {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return ParsedOutput::Empty;
        }

        if self.is_json_mode {
            self.parse_json(trimmed)
        } else {
            self.parse_terminal(raw)
        }
    }

    fn parse_json(&self, line: &str) -> ParsedOutput {
        match serde_json::from_str::<JsonOutputEvent>(line) {
            Ok(event) => ParsedOutput::JsonEvent(event),
            Err(_) => ParsedOutput::Text(line.to_string()),
        }
    }

    fn parse_terminal(&self, raw: &str) -> ParsedOutput {
        let stripped = strip_ansi(raw);
        let trimmed = stripped.trim();

        if trimmed.is_empty() {
            return ParsedOutput::Empty;
        }

        ParsedOutput::Text(trimmed.to_string())
    }

    pub fn is_completion_signal(&self, output: &ParsedOutput) -> bool {
        matches!(
            output,
            ParsedOutput::JsonEvent(JsonOutputEvent::Result { .. })
        )
    }

    pub fn extract_text(&self, output: &ParsedOutput) -> Option<String> {
        match output {
            ParsedOutput::Text(t) => Some(t.clone()),
            ParsedOutput::JsonEvent(JsonOutputEvent::Assistant { content }) => {
                let texts: Vec<&str> = content
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect();

                if texts.is_empty() {
                    None
                } else {
                    Some(texts.join(""))
                }
            }
            ParsedOutput::JsonEvent(JsonOutputEvent::Result {
                result: Some(r), ..
            }) => Some(r.clone()),
            _ => None,
        }
    }

    pub fn extract_session_id(&self, output: &ParsedOutput) -> Option<String> {
        match output {
            ParsedOutput::JsonEvent(JsonOutputEvent::Result {
                session_id: Some(id),
                ..
            }) => Some(id.clone()),
            _ => None,
        }
    }
}

fn strip_ansi(input: &str) -> String {
    let stripped = strip_ansi_escapes::strip(input.as_bytes());
    String::from_utf8_lossy(&stripped).to_string()
}
