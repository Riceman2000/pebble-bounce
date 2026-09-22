use base64::Engine as _;
use serde::Serialize;
use worker::wasm_bindgen::JsValue;
use worker::*;

/// Secret holding the URL the JSON is bounced to.
const URL_SECRET: &str = "BOUNCE_URL";

/// Largest request body accepted, checked before any of it is buffered.
const MAX_BODY_BYTES: usize = 25 * 1024 * 1024;

/// Response headers carried back from the bounce so that a rejection downstream
/// (a 401 in particular) reaches Pebble unchanged. `Location` is among them
/// because a redirect is reported rather than followed.
const PASSTHROUGH_HEADERS: [&str; 3] = ["Content-Type", "WWW-Authenticate", "Location"];

/// The `multipart/form-data` body Pebble's webhook posts, re-encoded as JSON.
///
/// See <https://help.repebble.com/en/articles/15724406-index-advanced-features-mcp-webhook>:
/// `recordedAt` and `client` are always sent, `transcription` and `audio` only
/// when the webhook is configured for text / audio (or both).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Payload {
    /// Recording timestamp as milliseconds since the unix epoch.
    recorded_at: Option<String>,
    /// Currently always the text `ring`.
    client: Option<String>,
    transcription: Option<String>,
    audio: Option<Audio>,
}

/// The `audio` part: M4A bytes, base64 encoded so they fit in JSON.
#[derive(Serialize)]
struct Audio {
    name: String,
    #[serde(rename = "type")]
    content_type: String,
    size: usize,
    data: String,
}

#[event(fetch)]
async fn fetch(mut req: Request, env: Env, _ctx: Context) -> Result<Response> {
    if req.method() != Method::Post {
        return Response::error("Expected POST", 405);
    }

    // Pebble sends whatever `Authorization` header the webhook is configured
    // with; it is handed straight to the bounce rather than checked here. Its
    // presence is checked before the body is read, so an unauthorized request
    // costs nothing.
    let authorization = match req.headers().get("Authorization")? {
        Some(authorization) if !authorization.is_empty() => authorization,
        _ => return Response::error("Missing Authorization header", 401),
    };

    // Gate on total request size
    match req
        .headers()
        .get("Content-Length")?
        .and_then(|len| len.parse::<usize>().ok())
    {
        Some(len) if len > MAX_BODY_BYTES => {
            return Response::error(format!("Body exceeds {MAX_BODY_BYTES} bytes"), 413)
        }
        Some(_) => {}
        None => return Response::error("Content-Length required", 411),
    }

    let url = match env.secret(URL_SECRET) {
        Ok(url) => url.to_string(),
        Err(_) => return Response::error(format!("Secret {URL_SECRET} is not configured"), 500),
    };

    let form = match req.form_data().await {
        Ok(form) => form,
        Err(e) => return Response::error(format!("Expected multipart/form-data: {e}"), 400),
    };

    let audio = match form.get("audio") {
        Some(FormEntry::File(file)) => Some(Audio {
            name: file.name(),
            content_type: file.type_(),
            size: file.size(),
            data: base64::engine::general_purpose::STANDARD.encode(file.bytes().await?),
        }),
        // Not a file part, but echo whatever was sent under that name anyway.
        Some(FormEntry::Field(field)) => Some(Audio {
            name: String::new(),
            content_type: String::new(),
            size: field.len(),
            data: base64::engine::general_purpose::STANDARD.encode(field),
        }),
        None => None,
    };

    let payload = serde_json::to_string(&Payload {
        recorded_at: form.get_field("recordedAt"),
        client: form.get_field("client"),
        transcription: form.get_field("transcription"),
        audio,
    })?;

    let headers = Headers::new();
    headers.set("Content-Type", "application/json")?;
    headers.set("Authorization", &authorization)?;

    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        // Following a redirect would replay the recording to whatever host the
        // destination names, so hand the 3xx back instead.
        .with_redirect(RequestRedirect::Manual)
        .with_body(Some(JsValue::from_str(&payload)));

    let mut bounced = match Fetch::Request(Request::new_with_init(&url, &init)?)
        .send()
        .await
    {
        Ok(response) => response,
        Err(e) => {
            // Logged rather than returned: the error text is not ours to leak.
            console_error!("bounce to {URL_SECRET} failed: {e}");
            return Response::error("Bounce failed", 502);
        }
    };

    // Hand the reply straight back to Pebble, status and all, so an auth
    // failure downstream shows up as that same failure upstream.
    let status = bounced.status_code();
    let passthrough: Vec<(&str, String)> = PASSTHROUGH_HEADERS
        .iter()
        .filter_map(|name| Some((*name, bounced.headers().get(name).ok()??)))
        .collect();
    let body = bounced.bytes().await.unwrap_or_default();

    let mut res = Response::from_bytes(body)?.with_status(status);
    for (name, value) in passthrough {
        res.headers_mut().set(name, &value)?;
    }
    // Ask browsers not to sniff our info pretty-please
    res.headers_mut().set("X-Content-Type-Options", "nosniff")?;
    Ok(res)
}
