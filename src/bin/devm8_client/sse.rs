use anyhow::Result;
use devm8::api::protocol::AskEvent;
use futures_util::StreamExt;

/// Minimal SSE consumer: reqwest gives us a raw byte stream, so we buffer and
/// split on the blank-line event separator ourselves rather than pulling in a
/// dedicated SSE client crate for this one use.
pub async fn for_each_event(
    resp: reqwest::Response,
    mut on_event: impl FnMut(&AskEvent),
) -> Result<()> {
    let mut buf = String::new();
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        buf.push_str(&String::from_utf8_lossy(&chunk?));

        while let Some(pos) = buf.find("\n\n") {
            let raw_event: String = buf.drain(..pos + 2).collect();
            for line in raw_event.lines() {
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let data = data.strip_prefix(' ').unwrap_or(data);
                let Ok(event) = serde_json::from_str::<AskEvent>(data) else {
                    continue;
                };
                let is_terminal = matches!(event, AskEvent::Done | AskEvent::Error { .. });
                on_event(&event);
                if is_terminal {
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}
