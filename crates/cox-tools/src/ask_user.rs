//! `ask_user`: the model asks the person a question and the turn blocks
//! until they answer (plan.md T3.8, §1.11). Separate from the other tools
//! because it is the only one whose "I/O" is a surface: the TUI answers
//! over a channel, a headless run answers with `--answer` or fails.

use async_trait::async_trait;
use cox_protocol::{CallId, Concurrency, Risk, Tool, ToolCx, ToolError, ToolOutput, ToolSpec};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};

use crate::write::str_field;

/// One question on its way to the surface; drop `reply` to dismiss it.
pub struct Question {
    /// The tool call asking.
    pub call: CallId,
    /// The question text.
    pub question: String,
    /// Suggested answers, possibly empty.
    pub options: Vec<String>,
    /// Where the answer goes.
    pub reply: oneshot::Sender<String>,
}

/// Where answers come from.
pub enum Answers {
    /// Headless: `--answer` text, or nothing (every question fails).
    Fixed(Option<String>),
    /// Interactive: the surface receives each `Question` and replies.
    Surface(mpsc::Sender<Question>),
}

pub struct AskUserTool {
    answers: Answers,
}

impl AskUserTool {
    pub fn new(answers: Answers) -> Self {
        Self { answers }
    }
}

#[async_trait]
impl Tool for AskUserTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "ask_user".to_string(),
            description: "Ask the user one question and wait for the answer. Use it only \
                when you cannot proceed without a decision that is theirs to make; do not \
                use it to confirm work you can verify yourself. Pass `question` and, when \
                the choice is between a few concrete alternatives, `options`. In a headless \
                run this returns the `--answer` text or an error."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "question": {"type": "string"},
                    "options": {"type": "array", "items": {"type": "string"}}
                },
                "required": ["question"]
            }),
            deferred: true,
            risk: Risk::ReadOnly,
            concurrency: Concurrency::Exclusive,
        }
    }

    fn subject(&self, input: &Value) -> String {
        input
            .get("question")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    async fn call(&self, input: Value, cx: &ToolCx) -> Result<ToolOutput, ToolError> {
        let question = str_field(&input, "question")?;
        let options: Vec<String> = input
            .get("options")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let answer = match &self.answers {
            Answers::Fixed(Some(answer)) => answer.clone(),
            Answers::Fixed(None) => {
                return Err(ToolError::Denied {
                    why: "no one can answer: this run is headless; pass --answer or run \
                          interactively"
                        .into(),
                });
            }
            Answers::Surface(tx) => {
                let (reply, answered) = oneshot::channel();
                let sent = tx
                    .send(Question {
                        call: cx.call,
                        question: question.clone(),
                        options: options.clone(),
                        reply,
                    })
                    .await;
                if sent.is_err() {
                    return Err(ToolError::Denied {
                        why: "no surface is listening for questions".into(),
                    });
                }
                tokio::select! {
                    // Cancel wins over a reply that lands in the same tick.
                    biased;
                    _ = cx.cancel.cancelled() => return Err(ToolError::Cancelled),
                    answer = answered => answer.map_err(|_| ToolError::Denied {
                        why: "the question was dismissed without an answer".into(),
                    })?,
                }
            }
        };
        Ok(ToolOutput {
            text: answer.clone(),
            is_error: false,
            diff: None,
            structured: Some(json!({
                "question": question,
                "options": options,
                "answer": answer,
            })),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use cox_protocol::{
        Archive, ArchiveId, ArchivePut, SandboxMode, SandboxPolicy, SessionId, StoreError,
    };
    use tokio_util::sync::CancellationToken;

    use super::*;

    struct NoopArchive;

    #[async_trait]
    impl Archive for NoopArchive {
        async fn put(&self, _put: ArchivePut) -> Result<ArchiveId, StoreError> {
            Ok(ArchiveId::new())
        }
        async fn get(&self, _id: &ArchiveId) -> Result<Vec<u8>, StoreError> {
            Ok(Vec::new())
        }
    }

    fn cx(cancel: CancellationToken) -> ToolCx {
        let (tx, _rx) = mpsc::channel(1);
        crate::tool_cx(
            vec![PathBuf::from("/tmp")],
            PathBuf::from("/tmp"),
            SandboxPolicy {
                mode: SandboxMode::ReadOnly,
                network: false,
                writable: vec![],
                readonly_in_workspace: vec![],
                linux_backend: Default::default(),
            },
            Arc::new(NoopArchive),
            cancel,
            tx,
            SessionId::new(),
            CallId::new(),
        )
    }

    #[tokio::test]
    async fn ask_user_headless_returns_the_fixed_answer_or_an_error() {
        let q = json!({"question": "which?", "options": ["a", "b"]});
        let out = AskUserTool::new(Answers::Fixed(Some("a".into())))
            .call(q.clone(), &cx(CancellationToken::new()))
            .await
            .expect("answered");
        assert_eq!(out.text, "a");
        let err = AskUserTool::new(Answers::Fixed(None))
            .call(q, &cx(CancellationToken::new()))
            .await
            .expect_err("headless without --answer");
        assert!(matches!(err, ToolError::Denied { .. }));
    }

    #[tokio::test]
    async fn ask_user_surface_reply_is_the_result_and_cancel_unblocks() {
        let (tx, mut rx) = mpsc::channel(1);
        let tool = AskUserTool::new(Answers::Surface(tx));
        let surface = tokio::spawn(async move {
            let q = rx.recv().await.expect("question");
            assert_eq!(q.question, "name?");
            let _ = q.reply.send("cox".into());
        });
        let out = tool
            .call(json!({"question": "name?"}), &cx(CancellationToken::new()))
            .await
            .expect("answered");
        assert_eq!(out.text, "cox");
        surface.await.expect("surface");

        let (tx, mut rx) = mpsc::channel(1);
        let tool = AskUserTool::new(Answers::Surface(tx));
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        tokio::spawn(async move {
            let _q = rx.recv().await;
            trigger.cancel();
        });
        let err = tool
            .call(json!({"question": "wait?"}), &cx(cancel))
            .await
            .expect_err("cancelled");
        assert!(matches!(err, ToolError::Cancelled));
    }
}
